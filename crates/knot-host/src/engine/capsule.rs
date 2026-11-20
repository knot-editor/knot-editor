//! Persistent isolate capsules and the turns that run inside them.

use super::*;

/// One persistent, movable V8 isolate and context for an extension lifetime.
///
/// All state attached to the shared isolate is `Send`; this is the safety
/// condition required by `v8::Isolate::try_into_shared`.
pub struct RuntimeCapsule {
    key: ExtensionKey,
    pub data: Mutex<Option<IsolateData>>,
    pub local_state: Arc<RuntimeLocalState>,
    pub(super) heap_state: Box<HeapLimitState>,
    pub status: Arc<AtomicU8>,
    /// The root evaluating the package or config entry, until it settles.
    startup_root: Mutex<Option<RootId>>,
}

impl RuntimeCapsule {
    #[cfg(any(test, feature = "test-support"))]
    pub fn new(key: ExtensionKey, config: IsolateConfig) -> Result<Self, RuntimeError> {
        Self::new_with_graph(key, config, None)
    }

    pub(super) fn new_with_graph(
        key: ExtensionKey,
        config: IsolateConfig,
        graph: Option<&ModuleGraph>,
    ) -> Result<Self, RuntimeError> {
        initialize();
        let status = Arc::new(AtomicU8::new(ACTIVE));
        let local_state = Arc::new(RuntimeLocalState::new(
            key,
            config.name.clone(),
            graph,
            Arc::clone(&status),
        ));
        let mut isolate =
            v8::Isolate::new(v8::CreateParams::default().heap_limits(0, config.heap_limit_bytes));
        isolate.set_microtasks_policy(v8::MicrotasksPolicy::Explicit);
        isolate.set_promise_reject_callback(promise_reject_callback);
        isolate.set_host_import_module_dynamically_callback(reject_dynamic_import);
        isolate.set_slot(Arc::clone(&local_state));
        let handle = isolate.thread_safe_handle();
        let mut heap_state = Box::new(HeapLimitState {
            status: Arc::clone(&status),
            isolate: handle,
        });
        isolate.add_near_heap_limit_callback(
            near_heap_limit_callback,
            ptr::from_mut(heap_state.as_mut()).cast::<c_void>(),
        );
        let context = {
            let scope = pin!(v8::HandleScope::new(&mut isolate));
            let mut scope = scope.init();
            let context = v8::Context::new(&scope, Default::default());
            let context_handle = v8::Global::new(&scope, context);
            let scope = &mut v8::ContextScope::new(&mut scope, context);
            initialize_extension_context(scope, &local_state).map(|()| context_handle)
        };
        let context = match context {
            Ok(context) => context,
            Err(error) => {
                clear_module_registry(&local_state);
                isolate.remove_near_heap_limit_callback(near_heap_limit_callback, 0);
                let _ = isolate.remove_slot::<Arc<RuntimeLocalState>>();
                drop(isolate);
                return Err(error);
            }
        };
        // SAFETY: the context, callback data, and isolate slot above contain
        // only owned Send state. No scoped handles or references survive.
        let isolate = match unsafe { isolate.try_into_shared() } {
            Ok(isolate) => isolate,
            Err(error) => {
                let message = format!("V8 isolate could not become shared: {error}");
                let mut isolate = error.into_isolate();
                clear_module_registry(&local_state);
                isolate.remove_near_heap_limit_callback(near_heap_limit_callback, 0);
                let _ = isolate.remove_slot::<Arc<RuntimeLocalState>>();
                drop(context);
                drop(isolate);
                return Err(RuntimeError::fatal(RuntimeErrorKind::Disposed, message));
            }
        };

        Ok(Self {
            key,
            data: Mutex::new(Some(IsolateData { isolate, context })),
            local_state,
            heap_state,
            status,
            startup_root: Mutex::new(None),
        })
    }

    pub fn key(&self) -> ExtensionKey {
        self.key
    }

    /// Reads the isolate's heap statistics between turns. A turn holds the
    /// isolate data for its whole duration, so a running or disposed isolate
    /// reports `None` instead of waiting.
    pub fn heap_usage(&self) -> Option<HeapUsage> {
        let data = self.data.try_lock().ok()?;
        let data = data.as_ref()?;
        let statistics = data.isolate.lock().get_heap_statistics();
        Some(HeapUsage {
            used: statistics.used_heap_size(),
            total: statistics.total_heap_size(),
            external: statistics.external_memory(),
        })
    }

    /// Asks V8 to collect garbage fully and return free pages, from the worker
    /// that just finished one of this isolate's turns.
    fn release_memory(&self) {
        if self.status.load(Ordering::Acquire) != ACTIVE {
            return;
        }
        let data = self.data.lock().unwrap();
        if let Some(data) = data.as_ref() {
            data.isolate.lock().low_memory_notification();
        }
    }

    pub fn termination_handle(&self) -> TerminationHandle {
        TerminationHandle {
            isolate: self.heap_state.isolate.clone(),
            status: Arc::clone(&self.status),
            wake: None,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn execute(
        &self,
        source_name: &str,
        source: &str,
    ) -> Result<ExecutionReport, RuntimeError> {
        if self.status.load(Ordering::Acquire) != ACTIVE {
            return Err(self.status_error());
        }
        let data = self.data.lock().unwrap();
        let data = data.as_ref().ok_or_else(RuntimeError::disposed)?;
        self.local_state.rejections.lock().unwrap().clear();
        let mut locker = data.isolate.lock();
        let scope = pin!(v8::HandleScope::new(&mut *locker));
        let mut scope = scope.init();
        let context = v8::Local::new(&scope, &data.context);
        let scope = &mut v8::ContextScope::new(&mut scope, context);
        v8::tc_scope!(let try_catch, scope);

        let name = v8::String::new(try_catch, source_name).ok_or_else(RuntimeError::disposed)?;
        let source_text = v8::String::new(try_catch, source).ok_or_else(RuntimeError::disposed)?;
        let origin = v8::ScriptOrigin::new(
            try_catch,
            name.into(),
            0,
            0,
            false,
            0,
            None,
            false,
            false,
            false,
            None,
        );
        let Some(script) = v8::Script::compile(try_catch, source_text, Some(&origin)) else {
            let error = if try_catch.has_terminated()
                || self.status.load(Ordering::Acquire) != ACTIVE
            {
                self.status_error()
            } else {
                runtime_error_from_try_catch(try_catch, RuntimeErrorKind::Compilation, source_name)
            };
            try_catch.reset();
            try_catch.perform_microtask_checkpoint();
            if try_catch.has_terminated() || self.status.load(Ordering::Acquire) != ACTIVE {
                return Err(self.status_error());
            }
            return Err(error);
        };
        let Some(value) = script.run(try_catch) else {
            let error = self.execution_error(try_catch, source_name);
            if !error.is_fatal() {
                try_catch.reset();
                try_catch.perform_microtask_checkpoint();
                if try_catch.has_terminated() || self.status.load(Ordering::Acquire) != ACTIVE {
                    return Err(self.status_error());
                }
            }
            return Err(error);
        };
        let value = value.to_rust_string_lossy(try_catch);
        try_catch.perform_microtask_checkpoint();
        if try_catch.has_terminated() || self.status.load(Ordering::Acquire) != ACTIVE {
            return Err(self.status_error());
        }
        let mut rejections = {
            let mut queued = self.local_state.rejections.lock().unwrap();
            std::mem::take(&mut *queued)
        };
        if !rejections.is_empty() {
            let rejection = rejections.remove(0);
            let mut message = rejection.message;
            for additional in &rejections {
                message.push('\n');
                message.push_str(&additional.message);
            }
            return Err(RuntimeError {
                kind: RuntimeErrorKind::Rejection,
                message: message.into(),
                source: rejection
                    .source
                    .or_else(|| Some(source_name.to_owned()))
                    .map(Into::into),
                line: rejection.line,
                column: rejection.column,
                stack: rejection.stack.map(Into::into),
            });
        }

        Ok(ExecutionReport {
            value: value.into(),
            worker: std::thread::current().id(),
        })
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn execute_fixture_module(
        &self,
        specifier: &str,
        source: &str,
    ) -> Result<ExecutionReport, RuntimeError> {
        let specifier = validate_root_module_specifier(specifier)?;
        if self.status.load(Ordering::Acquire) != ACTIVE {
            return Err(self.status_error());
        }
        let data = self.data.lock().unwrap();
        let data = data.as_ref().ok_or_else(RuntimeError::disposed)?;
        self.local_state.rejections.lock().unwrap().clear();
        let mut locker = data.isolate.lock();
        let scope = pin!(v8::HandleScope::new(&mut *locker));
        let mut scope = scope.init();
        let context = v8::Local::new(&scope, &data.context);
        let scope = &mut v8::ContextScope::new(&mut scope, context);
        let result = evaluate_fixture_module(
            scope,
            &self.local_state,
            &specifier,
            Some(Arc::from(source)),
            false,
        );
        if self.status.load(Ordering::Acquire) != ACTIVE {
            return Err(self.status_error());
        }
        result.map(|value| ExecutionReport {
            value: value.into(),
            worker: std::thread::current().id(),
        })
    }

    #[cfg(any(test, feature = "test-support"))]
    fn start_script_turn(
        &self,
        root: RootId,
        source_name: Arc<str>,
        source: Arc<str>,
        result: ResultSender<ExecutionReport>,
    ) -> TurnOutcome {
        self.run_root_turn(root, source_name, result, RootEvaluation::Script { source })
    }

    #[cfg(any(test, feature = "test-support"))]
    fn start_module_turn(
        &self,
        root: RootId,
        specifier: Arc<str>,
        source: Arc<str>,
        result: ResultSender<ExecutionReport>,
    ) -> TurnOutcome {
        let canonical = match validate_root_module_specifier(&specifier) {
            Ok(specifier) => Arc::<str>::from(specifier),
            Err(error) => {
                let _ = result.send(Err(error));
                return TurnOutcome::idle();
            }
        };
        self.run_root_turn(
            root,
            canonical,
            result,
            RootEvaluation::Module {
                source: Some(source),
            },
        )
    }

    /// Record an uncaught failure of a callback no caller awaits, with the
    /// package, the script position, and the JavaScript stack that locate it.
    fn report_failure(&self, label: &str, error: &RuntimeError) {
        tracing::error!(
            target: "knot::extensions",
            package = self.local_state.name.as_deref().unwrap_or("unknown"),
            source = error.source().unwrap_or("unknown"),
            line = error.line(),
            stack = %error.stack().unwrap_or("none"),
            "{label} failed: {error}",
        );
    }

    fn start_package_turn(
        &self,
        root: RootId,
        entry: Arc<str>,
        result: ResultSender<ExecutionReport>,
    ) -> TurnOutcome {
        self.run_root_turn(root, entry, result, RootEvaluation::Module { source: None })
    }

    fn start_notification_turn(
        &self,
        root: RootId,
        subscription: u64,
        notification: ListenerNotification,
        enqueued_at: Instant,
    ) -> TurnOutcome {
        let (callback, source, label) = match &notification {
            ListenerNotification::Buffer(_) => {
                self.local_state.buffer_change_started(enqueued_at);
                (
                    RuntimeCallback::BufferChange,
                    "knot:buffer-change",
                    "buffer-change listener",
                )
            }
            ListenerNotification::Application(_) => (
                RuntimeCallback::ApplicationChange,
                "knot:application-change",
                "application-change listener",
            ),
            ListenerNotification::Setting(_) => (
                RuntimeCallback::SettingChange,
                "knot:setting-change",
                "setting-change listener",
            ),
            ListenerNotification::Watch(_) => (
                RuntimeCallback::WatchChange,
                "knot:watch-change",
                "watch-change listener",
            ),
            ListenerNotification::Editor(_) => (
                RuntimeCallback::EditorEvent,
                "knot:editor-event",
                "editor listener",
            ),
            ListenerNotification::Timer => (RuntimeCallback::Timer, "knot:timer", "timer callback"),
        };
        if subscription > MAX_JS_INTEGER
            || matches!(&notification, ListenerNotification::Buffer(change) if change.buffer.value() > MAX_JS_INTEGER)
        {
            tracing::warn!(target: "knot::extensions", "listener identity cannot be represented exactly");
            return TurnOutcome::idle();
        }
        if self.status.load(Ordering::Acquire) != ACTIVE {
            return fatal_or_completed(&self.status_error());
        }
        let data = self.data.lock().unwrap();
        let Some(data) = data.as_ref() else {
            return TurnOutcome::Fatal(Failure::new("extension isolate is disposed"));
        };
        self.local_state.rejections.lock().unwrap().clear();
        let mut locker = data.isolate.lock();
        let scope = pin!(v8::HandleScope::new(&mut *locker));
        let mut scope = scope.init();
        let context = v8::Local::new(&scope, &data.context);
        let scope = &mut v8::ContextScope::new(&mut scope, context);
        v8::tc_scope!(let try_catch, scope);
        enter_root_frame(try_catch, root);
        let evaluation = (|| {
            let function = self.local_state.callback(try_catch, callback)?;
            let subscription = v8::Number::new(try_catch, subscription as f64);
            let change = match notification {
                ListenerNotification::Buffer(change) => buffer_change_to_v8(try_catch, change),
                ListenerNotification::Application(event) => {
                    application_event_to_v8(try_catch, &event)
                }
                ListenerNotification::Setting(key) => v8::String::new(try_catch, &key)
                    .map(Into::into)
                    .ok_or("UnsupportedOperation"),
                ListenerNotification::Watch(uris) => {
                    let elements: Option<Vec<v8::Local<v8::Value>>> = uris
                        .iter()
                        .map(|uri| v8::String::new(try_catch, uri).map(Into::into))
                        .collect();
                    elements
                        .map(|elements| v8::Array::new_with_elements(try_catch, &elements).into())
                        .ok_or("UnsupportedOperation")
                }
                ListenerNotification::Editor(event) => v8::String::new(try_catch, event.name())
                    .map(Into::into)
                    .ok_or("UnsupportedOperation"),
                ListenerNotification::Timer => Ok(v8::undefined(try_catch).into()),
            }
            .ok()?;
            function.call(
                try_catch,
                v8::undefined(try_catch).into(),
                &[subscription.into(), change],
            )
        })();
        let Some(value) = evaluation else {
            let error = self.execution_error(try_catch, source);
            leave_frame(try_catch);
            if !error.is_fatal() {
                self.report_failure(label, &error);
                try_catch.reset();
                self.reject_pending_for_root(try_catch, root);
            }
            return fatal_or_completed(&error);
        };
        let promise = if let Ok(promise) = v8::Local::<v8::Promise>::try_from(value) {
            promise
        } else {
            let resolver = v8::PromiseResolver::new(try_catch).expect("V8 resolver allocation");
            resolver.resolve(try_catch, value);
            resolver.get_promise(try_catch)
        };
        promise.mark_as_handled();
        self.local_state.active.lock().unwrap().insert(
            root,
            ActiveExecution::new(
                v8::Global::new(try_catch, promise),
                source.into(),
                ActiveCompletion::Detached(label),
            ),
        );
        try_catch.perform_microtask_checkpoint();
        leave_frame(try_catch);
        self.settle_turn(try_catch, Some(root))
    }

    fn start_command_turn(
        &self,
        root: RootId,
        invocation: CommandInvocation,
        active_buffer: Option<BufferHandle>,
        result: ResultSender<CommandOutcome>,
    ) -> TurnOutcome {
        if invocation.id.value() > MAX_JS_INTEGER
            || invocation.registration.value() > MAX_JS_INTEGER
            || active_buffer.is_some_and(|buffer| buffer.value() > MAX_JS_INTEGER)
            || invocation
                .capture
                .as_ref()
                .is_some_and(|capture| capture.authority.value() > MAX_JS_INTEGER)
        {
            let _ = result.send(Ok(CommandOutcome::HandlerFailure {
                message: "native command identity cannot be represented exactly".into(),
            }));
            return TurnOutcome::idle();
        }
        if self
            .local_state
            .cancelled_commands
            .lock()
            .unwrap()
            .remove(&invocation.id)
        {
            let _ = result.send(Ok(CommandOutcome::Cancelled));
            return TurnOutcome::idle();
        }
        if self.status.load(Ordering::Acquire) != ACTIVE {
            let error = self.status_error();
            let _ = result.send(Err(error.clone()));
            return fatal_or_completed(&error);
        }
        let data = self.data.lock().unwrap();
        let Some(data) = data.as_ref() else {
            let error = RuntimeError::disposed();
            let _ = result.send(Err(error.clone()));
            return TurnOutcome::Fatal(Failure::new(error.to_string()));
        };
        self.local_state.rejections.lock().unwrap().clear();
        let mut locker = data.isolate.lock();
        let scope = pin!(v8::HandleScope::new(&mut *locker));
        let mut scope = scope.init();
        let context = v8::Local::new(&scope, &data.context);
        let scope = &mut v8::ContextScope::new(&mut scope, context);
        v8::tc_scope!(let try_catch, scope);
        enter_root_frame(try_catch, root);
        let evaluated = (|| {
            let function = self
                .local_state
                .callback(try_catch, RuntimeCallback::Command)?;
            let active_buffer = active_buffer.map_or_else(
                || v8::null(try_catch).into(),
                |buffer| v8::Number::new(try_catch, buffer.value() as f64).into(),
            );
            let capture = match &invocation.capture {
                Some(capture) => {
                    let json = serde_json::to_string(capture).ok()?;
                    let json = v8::String::new(try_catch, &json)?;
                    v8::json::parse(try_catch, json)?
                }
                None => v8::null(try_catch).into(),
            };
            let arguments = command_argument_to_v8(try_catch, &invocation.arguments).ok()?;
            function.call(
                try_catch,
                v8::undefined(try_catch).into(),
                &[
                    v8::Number::new(try_catch, invocation.id.value() as f64).into(),
                    v8::Number::new(try_catch, invocation.registration.value() as f64).into(),
                    active_buffer,
                    arguments,
                    capture,
                ],
            )
        })();
        let Some(value) = evaluated else {
            let error = self.execution_error(try_catch, "knot:command");
            leave_frame(try_catch);
            let _ = result.send(Err(error.clone()));
            if !error.is_fatal() {
                try_catch.reset();
                self.reject_pending_for_root(try_catch, root);
            }
            return fatal_or_completed(&error);
        };
        let promise = if let Ok(promise) = v8::Local::<v8::Promise>::try_from(value) {
            promise
        } else {
            let resolver = v8::PromiseResolver::new(try_catch).expect("V8 resolver allocation");
            resolver.resolve(try_catch, value);
            resolver.get_promise(try_catch)
        };
        promise.mark_as_handled();
        #[cfg(any(test, feature = "test-support"))]
        if let Some(gate) = self.local_state.command_start_gate.lock().unwrap().take() {
            gate.wait();
            gate.wait();
        }
        let cancelled = {
            // Cancellation either sees the published command or is consumed here.
            let mut cancelled = self.local_state.cancelled_commands.lock().unwrap();
            let mut execution = ActiveExecution::new(
                v8::Global::new(try_catch, promise),
                "knot:command".into(),
                ActiveCompletion::Command(result),
            );
            execution.command = Some(invocation.id);
            self.local_state
                .active
                .lock()
                .unwrap()
                .insert(root, execution);
            cancelled.remove(&invocation.id)
        };
        if cancelled {
            self.cancel_command_in_scope(try_catch, invocation.id);
        }
        try_catch.perform_microtask_checkpoint();
        leave_frame(try_catch);
        self.settle_turn(try_catch, Some(root))
    }

    fn start_provider_turn(&self, root: RootId, call: ProviderCall) -> TurnOutcome {
        let exact = match &call {
            ProviderCall::Tree { request, .. } => {
                request.registration.value() <= MAX_JS_INTEGER
                    && request.generation <= MAX_JS_INTEGER
            }
            ProviderCall::Call { request, .. } => request.registration.value() <= MAX_JS_INTEGER,
        };
        if !exact {
            call.complete_with_error(RuntimeError::fatal(
                RuntimeErrorKind::Engine,
                "native provider coordinate cannot be represented exactly",
            ));
            return TurnOutcome::idle();
        }
        if call.cancelled() {
            call.complete_with_error(RuntimeError::fatal(
                RuntimeErrorKind::Cancelled,
                "semantic provider request was dropped",
            ));
            return TurnOutcome::idle();
        }
        if self.status.load(Ordering::Acquire) != ACTIVE {
            let error = self.status_error();
            call.complete_with_error(error.clone());
            return fatal_or_completed(&error);
        }
        let data = self.data.lock().unwrap();
        let Some(data) = data.as_ref() else {
            let error = RuntimeError::disposed();
            call.complete_with_error(error.clone());
            return TurnOutcome::Fatal(Failure::new(error.to_string()));
        };
        self.local_state.rejections.lock().unwrap().clear();
        let mut locker = data.isolate.lock();
        let scope = pin!(v8::HandleScope::new(&mut *locker));
        let mut scope = scope.init();
        let context = v8::Local::new(&scope, &data.context);
        let scope = &mut v8::ContextScope::new(&mut scope, context);
        v8::tc_scope!(let try_catch, scope);
        enter_root_frame(try_catch, root);
        let (source, evaluated) = match &call {
            ProviderCall::Tree { request, .. } => {
                let source = "knot:tree-provider";
                let value = (|| {
                    let function = self
                        .local_state
                        .callback(try_catch, RuntimeCallback::Tree)?;
                    let parent = request.parent_id.as_ref().map_or_else(
                        || Some(v8::null(try_catch).into()),
                        |parent| v8::String::new(try_catch, parent).map(Into::into),
                    )?;
                    let window = v8::String::new(try_catch, &request.window)?;
                    let instance = v8::String::new(try_catch, &request.view.value().to_string())?;
                    function.call(
                        try_catch,
                        v8::undefined(try_catch).into(),
                        &[
                            v8::Number::new(try_catch, request.registration.value() as f64).into(),
                            window.into(),
                            instance.into(),
                            parent,
                            v8::Number::new(try_catch, request.generation as f64).into(),
                        ],
                    )
                })();
                (source, value)
            }
            ProviderCall::Call { request, .. } => {
                let source = "knot:provider";
                let value = (|| {
                    let function = self
                        .local_state
                        .callback(try_catch, RuntimeCallback::ProviderCall)?;
                    let method = v8::String::new(try_catch, &request.method)?;
                    let argument = v8::String::new(try_catch, &request.argument)?;
                    let window = request.window.as_ref().map_or_else(
                        || Some(v8::null(try_catch).into()),
                        |window| v8::String::new(try_catch, window).map(Into::into),
                    )?;
                    function.call(
                        try_catch,
                        v8::undefined(try_catch).into(),
                        &[
                            v8::Number::new(try_catch, request.registration.value() as f64).into(),
                            method.into(),
                            argument.into(),
                            window,
                        ],
                    )
                })();
                (source, value)
            }
        };
        let Some(value) = evaluated else {
            let error = self.execution_error(try_catch, source);
            leave_frame(try_catch);
            call.complete_with_error(error.clone());
            if !error.is_fatal() {
                try_catch.reset();
                self.reject_pending_for_root(try_catch, root);
            }
            return fatal_or_completed(&error);
        };
        let promise = if let Ok(promise) = v8::Local::<v8::Promise>::try_from(value) {
            promise
        } else {
            let resolver = v8::PromiseResolver::new(try_catch).expect("V8 resolver allocation");
            resolver.resolve(try_catch, value);
            resolver.get_promise(try_catch)
        };
        promise.mark_as_handled();
        self.local_state.active.lock().unwrap().insert(
            root,
            ActiveExecution::new(
                v8::Global::new(try_catch, promise),
                source.into(),
                call.into_active_completion(),
            ),
        );
        try_catch.perform_microtask_checkpoint();
        leave_frame(try_catch);
        self.settle_turn(try_catch, Some(root))
    }

    fn cancel_provider_turn(&self, root: RootId) -> TurnOutcome {
        let data = self.data.lock().unwrap();
        let Some(data) = data.as_ref() else {
            return TurnOutcome::Fatal(Failure::new("extension isolate is disposed"));
        };
        let mut locker = data.isolate.lock();
        let scope = pin!(v8::HandleScope::new(&mut *locker));
        let mut scope = scope.init();
        let context = v8::Local::new(&scope, &data.context);
        let scope = &mut v8::ContextScope::new(&mut scope, context);
        self.reject_pending_for_root(scope, root);
        let active = self.local_state.active.lock().unwrap().remove(&root);
        let Some(active) = active else {
            return TurnOutcome::idle();
        };
        let error = RuntimeError::fatal(
            RuntimeErrorKind::Cancelled,
            "semantic provider request was dropped",
        );
        match active.completion {
            ActiveCompletion::Tree {
                result, completed, ..
            } => {
                completed.store(true, Ordering::Release);
                let _ = result.send(Err(error));
            }
            ActiveCompletion::Call { result, completed } => {
                completed.store(true, Ordering::Release);
                let _ = result.send(Err(error));
            }
            completion => {
                self.local_state.active.lock().unwrap().insert(
                    root,
                    ActiveExecution {
                        completion,
                        ..active
                    },
                );
                return TurnOutcome::Fatal(Failure::new(
                    "provider cancellation addressed a non-provider root",
                ));
            }
        }
        TurnOutcome::Completed {
            settled: vec![root],
        }
    }

    fn cancel_command_turn(&self, invocation: CommandInvocationId) -> TurnOutcome {
        let data = self.data.lock().unwrap();
        let Some(data) = data.as_ref() else {
            return TurnOutcome::Fatal(Failure::new("extension isolate is disposed"));
        };
        let mut locker = data.isolate.lock();
        let scope = pin!(v8::HandleScope::new(&mut *locker));
        let mut scope = scope.init();
        let context = v8::Local::new(&scope, &data.context);
        let scope = &mut v8::ContextScope::new(&mut scope, context);
        if self.cancel_command_in_scope(scope, invocation) {
            self.local_state
                .cancelled_commands
                .lock()
                .unwrap()
                .remove(&invocation);
        }
        self.settle_turn(scope, None)
    }

    /// Aborts a command's controller and rejects its pending contextual
    /// requests; false when no such command is running in the isolate.
    fn cancel_command_in_scope(
        &self,
        scope: &mut v8::PinScope<'_, '_>,
        invocation: CommandInvocationId,
    ) -> bool {
        let found = self
            .local_state
            .callback(scope, RuntimeCallback::CancelCommand)
            .and_then(|function| {
                function.call(
                    scope,
                    v8::undefined(scope).into(),
                    &[v8::Number::new(scope, invocation.value() as f64).into()],
                )
            })
            .is_some_and(|found| found.is_true());
        self.reject_pending_for_command(scope, invocation);
        scope.perform_microtask_checkpoint();
        found
    }

    fn run_root_turn(
        &self,
        root: RootId,
        source: Arc<str>,
        result: ResultSender<ExecutionReport>,
        evaluation: RootEvaluation,
    ) -> TurnOutcome {
        if self.status.load(Ordering::Acquire) != ACTIVE {
            let error = self.status_error();
            let _ = result.send(Err(error.clone()));
            return fatal_or_completed(&error);
        }
        let data = self.data.lock().unwrap();
        let Some(data) = data.as_ref() else {
            let error = RuntimeError::disposed();
            let _ = result.send(Err(error.clone()));
            return TurnOutcome::Fatal(Failure::new(error.to_string()));
        };
        self.local_state.rejections.lock().unwrap().clear();
        let mut locker = data.isolate.lock();
        let scope = pin!(v8::HandleScope::new(&mut *locker));
        let mut scope = scope.init();
        let context = v8::Local::new(&scope, &data.context);
        let scope = &mut v8::ContextScope::new(&mut scope, context);
        v8::tc_scope!(let try_catch, scope);
        enter_root_frame(try_catch, root);
        let evaluated = match evaluation {
            #[cfg(any(test, feature = "test-support"))]
            RootEvaluation::Script {
                source: script_source,
            } => {
                let name = v8::String::new(try_catch, &source).ok_or_else(RuntimeError::disposed);
                let source_text =
                    v8::String::new(try_catch, &script_source).ok_or_else(RuntimeError::disposed);
                name.and_then(|name| {
                    source_text.and_then(|source_text| {
                        let origin = v8::ScriptOrigin::new(
                            try_catch,
                            name.into(),
                            0,
                            0,
                            false,
                            0,
                            None,
                            false,
                            false,
                            false,
                            None,
                        );
                        let script = v8::Script::compile(try_catch, source_text, Some(&origin))
                            .ok_or_else(|| {
                                runtime_error_from_try_catch(
                                    try_catch,
                                    RuntimeErrorKind::Compilation,
                                    &source,
                                )
                            })?;
                        script
                            .run(try_catch)
                            .ok_or_else(|| self.execution_error(try_catch, &source))
                    })
                })
            }
            RootEvaluation::Module {
                source: module_source,
            } => (|| {
                if let Some(module_source) = module_source {
                    let mut modules = self.local_state.modules.lock().unwrap();
                    if modules.package_root.is_some() {
                        return Err(module_resolution_error(
                            &source,
                            "package module graph cannot change after load",
                        ));
                    }
                    if let Some(existing) = modules.sources.get(source.as_ref()) {
                        if existing.as_ref() != module_source.as_ref() {
                            return Err(module_resolution_error(
                                &source,
                                "fixture module source cannot change after registration",
                            ));
                        }
                    } else {
                        modules.sources.insert(source.to_string(), module_source);
                    }
                }
                let (module_count, resolution_count) = {
                    let modules = self.local_state.modules.lock().unwrap();
                    (modules.modules.len(), modules.resolutions.len())
                };
                let compiled = {
                    let mut modules = self.local_state.modules.lock().unwrap();
                    let blocked = blocked_post_init_import(&modules, &source);
                    compile_module_graph(try_catch, &mut modules, &source, blocked.as_deref())
                };
                let compiled = match compiled {
                    Ok(compiled) => compiled,
                    Err(failure) => {
                        let error = failure.report(try_catch, &source);
                        rollback_module_graph(&self.local_state, module_count, resolution_count);
                        return Err(error);
                    }
                };
                let module = v8::Local::new(try_catch, &compiled);
                if module.instantiate_module(try_catch, resolve_module_callback) != Some(true) {
                    let error = runtime_error_from_try_catch(
                        try_catch,
                        RuntimeErrorKind::ModuleResolution,
                        &source,
                    );
                    rollback_module_graph(&self.local_state, module_count, resolution_count);
                    return Err(error);
                }
                module.evaluate(try_catch).ok_or_else(|| {
                    runtime_error_from_try_catch(try_catch, RuntimeErrorKind::Exception, &source)
                })
            })(),
        };
        let value = match evaluated {
            Ok(value) => value,
            Err(error) => {
                leave_frame(try_catch);
                let _ = result.send(Err(error.clone()));
                self.reject_pending_for_root(try_catch, root);
                return fatal_or_completed(&error);
            }
        };
        let promise = if let Ok(promise) = v8::Local::<v8::Promise>::try_from(value) {
            promise
        } else {
            let resolver = v8::PromiseResolver::new(try_catch).expect("V8 resolver allocation");
            resolver.resolve(try_catch, value);
            resolver.get_promise(try_catch)
        };
        promise.mark_as_handled();
        self.local_state.active.lock().unwrap().insert(
            root,
            ActiveExecution::new(
                v8::Global::new(try_catch, promise),
                source,
                ActiveCompletion::Report(result),
            ),
        );
        try_catch.perform_microtask_checkpoint();
        leave_frame(try_catch);
        self.settle_turn(try_catch, Some(root))
    }

    fn resume_response_turn(&self, root: Option<RootId>, response: HostResponse) -> TurnOutcome {
        let data = self.data.lock().unwrap();
        let Some(data) = data.as_ref() else {
            return TurnOutcome::Fatal(Failure::new("extension isolate is disposed"));
        };
        let mut locker = data.isolate.lock();
        let scope = pin!(v8::HandleScope::new(&mut *locker));
        let mut scope = scope.init();
        let context = v8::Local::new(&scope, &data.context);
        let scope = &mut v8::ContextScope::new(&mut scope, context);
        let batch_id = *self.local_state.keybinding_batch_pending.lock().unwrap();
        if batch_id.is_some() && batch_id != Some(response.id) {
            self.local_state
                .deferred_responses
                .lock()
                .unwrap()
                .push(response);
            return TurnOutcome::idle();
        }
        let pending = self
            .local_state
            .pending
            .lock()
            .unwrap()
            .remove(&response.id);
        let Some(pending) = pending else {
            return TurnOutcome::Fatal(Failure::new("host response lost its pending request"));
        };
        if batch_id == Some(response.id) {
            *self.local_state.keybinding_batch_pending.lock().unwrap() = None;
            let error = match response.result {
                Ok(reply) if reply.operation == pending.operation => match reply.result {
                    Ok(Ok(knot_protocol::declarations::EncodedValue::Undefined)) => None,
                    Err(error) => Some(error.message),
                    _ => Some("invalid keybinding batch response".into()),
                },
                Err(error) => Some(format!("keybinding batch was rejected: {}", error.code)),
                _ => Some("invalid keybinding batch response".into()),
            };
            if let Some(message) = error {
                let source = pending.root.and_then(|root| {
                    self.local_state
                        .active
                        .lock()
                        .unwrap()
                        .get(&root)
                        .map(|active| active.source.clone())
                });
                let error = RuntimeError {
                    kind: RuntimeErrorKind::Exception,
                    message: message.into(),
                    source,
                    line: None,
                    column: None,
                    stack: None,
                };
                self.local_state.deferred_responses.lock().unwrap().clear();
                self.local_state
                    .keybinding_mutations
                    .lock()
                    .unwrap()
                    .clear();
                // The invocation whose turn queued the batch fails with it.
                let failed = pending.root.filter(|root| {
                    let execution = self.local_state.active.lock().unwrap().remove(root);
                    execution.is_some_and(|execution| {
                        self.fail_execution(execution, error.clone(), true);
                        true
                    })
                });
                if failed.is_none() {
                    self.report_failure("keybinding batch", &error);
                }
                return TurnOutcome::Completed {
                    settled: failed.into_iter().collect(),
                };
            }
            let deferred =
                std::mem::take(&mut *self.local_state.deferred_responses.lock().unwrap());
            for response in deferred {
                let Some(pending) = self
                    .local_state
                    .pending
                    .lock()
                    .unwrap()
                    .remove(&response.id)
                else {
                    let error = RuntimeError::fatal(
                        RuntimeErrorKind::Engine,
                        "deferred host response lost its pending request",
                    );
                    self.fail_all_active(error.clone());
                    return fatal_or_completed(&error);
                };
                Self::settle_host_response(scope, pending, response);
            }
        } else {
            Self::settle_host_response(scope, pending, response);
        }
        scope.perform_microtask_checkpoint();
        self.settle_turn(scope, root)
    }

    fn settle_host_response(
        scope: &mut v8::PinScope<'_, '_>,
        pending: PendingRequest,
        response: HostResponse,
    ) {
        let Some(resolver) = pending.resolver else {
            return;
        };
        let resolver = v8::Local::new(scope, &resolver);
        let value = match response.result {
            Ok(response) if pending.operation == response.operation => {
                declared_response_to_v8(scope, &response)
            }
            Ok(_) => Err(boundary_exception(
                scope,
                BoundaryError::Host(TransportError::UnsupportedOperation),
            )),
            Err(error) => Err(host_failure_exception(scope, &error)),
        };
        match value {
            Ok(value) => {
                resolver.resolve(scope, value);
            }
            Err(error) => {
                resolver.reject(scope, error);
            }
        }
    }

    fn finish_termination_turn(&self) -> TurnOutcome {
        let data = self.data.lock().unwrap();
        let Some(data) = data.as_ref() else {
            return TurnOutcome::Fatal(Failure::new("extension isolate is disposed"));
        };
        let mut locker = data.isolate.lock();
        let scope = pin!(v8::HandleScope::new(&mut *locker));
        let mut scope = scope.init();
        let context = v8::Local::new(&scope, &data.context);
        let scope = &mut v8::ContextScope::new(&mut scope, context);
        self.settle_turn(scope, None)
    }

    /// Ends a turn: reports unhandled rejections, queues pending keybinding
    /// changes, and settles every invocation whose promise has settled. An
    /// invocation completes when its promise does; requests it left in flight
    /// do not keep it alive.
    fn settle_turn(&self, scope: &mut v8::PinScope<'_, '_>, root: Option<RootId>) -> TurnOutcome {
        if self.status.load(Ordering::Acquire) != ACTIVE {
            let error = self.status_error();
            if let Some(root) = root {
                self.reject_pending_for_root(scope, root);
            }
            self.fail_all_active(error.clone());
            return TurnOutcome::Fatal(Failure::new(error.to_string()));
        }
        let source = root
            .and_then(|root| {
                self.local_state
                    .active
                    .lock()
                    .unwrap()
                    .get(&root)
                    .map(|active| active.source.clone())
            })
            .unwrap_or_else(|| "knot:turn".into());
        if let Some(rejection) = self.take_unhandled_rejection(&source) {
            let mut active = self.local_state.active.lock().unwrap();
            if let Some(execution) = root.and_then(|root| active.get_mut(&root)) {
                execution.rejection.get_or_insert(rejection);
            } else {
                drop(active);
                self.report_failure("unhandled rejection", &rejection);
            }
        }
        if self
            .local_state
            .keybinding_batch_pending
            .lock()
            .unwrap()
            .is_some()
        {
            return TurnOutcome::idle();
        }
        match self.flush_keybinding_mutations(root) {
            Ok(true) => return TurnOutcome::idle(),
            Ok(false) => {}
            Err(error) => {
                self.fail_all_active(error.clone());
                return fatal_or_completed(&error);
            }
        }
        let finished: Vec<(RootId, ActiveExecution)> = {
            let mut active = self.local_state.active.lock().unwrap();
            let roots: Vec<RootId> = active
                .iter()
                .filter(|(_, execution)| {
                    v8::Local::new(scope, &execution.promise).state() != v8::PromiseState::Pending
                })
                .map(|(root, _)| *root)
                .collect();
            roots
                .into_iter()
                .filter_map(|root| active.remove(&root).map(|execution| (root, execution)))
                .collect()
        };
        let mut settled = Vec::with_capacity(finished.len());
        for (root, execution) in finished {
            let rejected =
                v8::Local::new(scope, &execution.promise).state() == v8::PromiseState::Rejected;
            self.complete_execution(scope, execution);
            if rejected {
                self.reject_pending_for_root(scope, root);
            }
            settled.push(root);
        }
        TurnOutcome::Completed { settled }
    }

    /// Delivers the result of an invocation whose promise has settled.
    fn complete_execution(&self, scope: &mut v8::PinScope<'_, '_>, execution: ActiveExecution) {
        let promise = v8::Local::new(scope, &execution.promise);
        let value = promise.result(scope);
        if promise.state() == v8::PromiseState::Rejected {
            let message = v8::Exception::create_message(scope, value);
            let error = RuntimeError {
                kind: RuntimeErrorKind::Rejection,
                message: message.get(scope).to_rust_string_lossy(scope).into(),
                source: message
                    .get_script_resource_name(scope)
                    .filter(|value| !value.is_null_or_undefined())
                    .and_then(|value| value.to_string(scope))
                    .map(|value| value.to_rust_string_lossy(scope).into())
                    .or_else(|| Some(Arc::clone(&execution.source))),
                line: message
                    .get_line_number(scope)
                    .and_then(|line| u32::try_from(line).ok()),
                column: one_based_coordinate(message.get_start_column()),
                stack: rejection_stack(scope, value),
            };
            match execution.completion {
                ActiveCompletion::Report(sender) => {
                    let _ = sender.send(Err(error));
                }
                ActiveCompletion::Command(sender) => {
                    let _ = sender.send(Ok(CommandOutcome::HandlerFailure {
                        message: error.to_string(),
                    }));
                }
                ActiveCompletion::Tree {
                    request,
                    result,
                    completed,
                } => {
                    completed.store(true, Ordering::Release);
                    let _ = result.send(Ok(tree_failure(&request, &error)));
                }
                ActiveCompletion::Call { result, completed } => {
                    completed.store(true, Ordering::Release);
                    let _ = result.send(Ok(Err(error.to_string())));
                }
                ActiveCompletion::Detached(label) => {
                    self.report_failure(label, &error);
                }
            }
            return;
        }
        let rejection = execution.rejection;
        match execution.completion {
            ActiveCompletion::Report(sender) => {
                let text = value.to_rust_string_lossy(scope);
                let result = rejection.map_or_else(
                    || {
                        Ok(ExecutionReport {
                            value: text.into(),
                            worker: std::thread::current().id(),
                        })
                    },
                    Err,
                );
                let _ = sender.send(result);
            }
            ActiveCompletion::Command(sender) => {
                let result = rejection.map_or_else(|| command_outcome_from_v8(scope, value), Err);
                let _ = sender.send(result);
            }
            ActiveCompletion::Tree {
                request,
                result,
                completed,
            } => {
                let response = match rejection {
                    None => tree_response_from_v8(scope, request, value),
                    Some(error) => tree_failure(&request, &error),
                };
                completed.store(true, Ordering::Release);
                let _ = result.send(Ok(response));
            }
            ActiveCompletion::Call { result, completed } => {
                let response = match rejection {
                    None => provider_call_response_from_v8(scope, value),
                    Some(error) => Err(error.to_string()),
                };
                completed.store(true, Ordering::Release);
                let _ = result.send(Ok(response));
            }
            ActiveCompletion::Detached(label) => {
                if let Some(error) = rejection {
                    self.report_failure(label, &error);
                }
            }
        }
    }

    fn flush_keybinding_mutations(&self, root: Option<RootId>) -> Result<bool, RuntimeError> {
        let mutations = std::mem::take(&mut *self.local_state.keybinding_mutations.lock().unwrap());
        if mutations.is_empty() {
            return Ok(false);
        }
        let request_value = self
            .local_state
            .next_request
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| {
                RuntimeError::fatal(RuntimeErrorKind::Engine, "request identity exhausted")
            })?;
        let id = RequestId::new(request_value);
        let operation = knot_protocol::declarations::registry()
            .get("mutateKeybindings")
            .expect("native keymap handler")
            .request(knot_protocol::KeybindingArguments { mutations });
        self.local_state.pending.lock().unwrap().insert(
            id,
            PendingRequest {
                root,
                invocation: None,
                operation: operation.descriptor.name,
                resolver: None,
                response_queued: false,
                provider_mutation: None,
            },
        );
        *self.local_state.keybinding_batch_pending.lock().unwrap() = Some(id);
        let sender = self.local_state.event_sender.lock().unwrap().clone();
        let sent = sender.is_some_and(|sender| {
            sender
                .send(EngineEvent::Request(HostRequest {
                    extension: self.key.extension,
                    lifecycle: self.key.lifecycle,
                    id,
                    invocation: None,
                    operation,
                }))
                .is_ok()
        });
        if !sent {
            self.local_state.pending.lock().unwrap().remove(&id);
            *self.local_state.keybinding_batch_pending.lock().unwrap() = None;
            return Err(RuntimeError::fatal(
                RuntimeErrorKind::Engine,
                "keybinding batch could not reach the application",
            ));
        }
        Ok(true)
    }

    fn reject_pending_for_root(&self, scope: &mut v8::PinScope<'_, '_>, root: RootId) {
        let pending = {
            let mut requests = self.local_state.pending.lock().unwrap();
            let ids: Vec<_> = requests
                .iter()
                .filter_map(|(id, pending)| (pending.root == Some(root)).then_some(*id))
                .collect();
            ids.into_iter()
                .filter_map(|id| requests.remove(&id))
                .collect::<Vec<_>>()
        };
        for pending in pending {
            if let Some(resolver) = pending.resolver {
                let resolver = v8::Local::new(scope, &resolver);
                let cancelled =
                    boundary_exception(scope, BoundaryError::Host(TransportError::Cancelled));
                resolver.reject(scope, cancelled);
            }
        }
        scope.perform_microtask_checkpoint();
    }

    fn reject_pending_for_command(
        &self,
        scope: &mut v8::PinScope<'_, '_>,
        invocation: CommandInvocationId,
    ) {
        let pending = {
            let mut requests = self.local_state.pending.lock().unwrap();
            let ids: Vec<_> = requests
                .iter()
                .filter_map(|(id, pending)| (pending.invocation == Some(invocation)).then_some(*id))
                .collect();
            ids.into_iter()
                .filter_map(|id| {
                    if requests
                        .get(&id)
                        .is_some_and(|pending| pending.response_queued)
                    {
                        // Keep an admitted response attached until its queued turn consumes it.
                        requests
                            .get_mut(&id)
                            .and_then(|pending| pending.resolver.take())
                    } else {
                        requests.remove(&id).and_then(|pending| pending.resolver)
                    }
                })
                .collect::<Vec<_>>()
        };
        for resolver in pending {
            let resolver = v8::Local::new(scope, &resolver);
            let cancelled =
                boundary_exception(scope, BoundaryError::Host(TransportError::Cancelled));
            resolver.reject(scope, cancelled);
        }
        scope.perform_microtask_checkpoint();
    }

    fn take_unhandled_rejection(&self, fallback_source: &str) -> Option<RuntimeError> {
        let mut rejections = {
            let mut queued = self.local_state.rejections.lock().unwrap();
            std::mem::take(&mut *queued)
        };
        if rejections.is_empty() {
            return None;
        }
        let rejection = rejections.remove(0);
        let mut message = rejection.message;
        for additional in &rejections {
            message.push('\n');
            message.push_str(&additional.message);
        }
        Some(RuntimeError {
            kind: RuntimeErrorKind::Rejection,
            message: message.into(),
            source: rejection
                .source
                .or_else(|| Some(fallback_source.to_owned()))
                .map(Into::into),
            line: rejection.line,
            column: rejection.column,
            stack: rejection.stack.map(Into::into),
        })
    }

    /// Fails every unsettled invocation and forgets all in-flight host work.
    fn fail_all_active(&self, error: RuntimeError) {
        let executions: Vec<_> = self.local_state.active.lock().unwrap().drain().collect();
        for (_, execution) in executions {
            self.fail_execution(execution, error.clone(), false);
        }
        self.local_state.pending.lock().unwrap().clear();
        self.local_state
            .keybinding_mutations
            .lock()
            .unwrap()
            .clear();
        *self.local_state.keybinding_batch_pending.lock().unwrap() = None;
        self.local_state.deferred_responses.lock().unwrap().clear();
    }

    /// Delivers `error` as the result of one invocation; a command reports it
    /// as its own handler failure when `command_failure` is set.
    fn fail_execution(
        &self,
        execution: ActiveExecution,
        error: RuntimeError,
        command_failure: bool,
    ) {
        match execution.completion {
            ActiveCompletion::Report(result) => {
                let _ = result.send(Err(error));
            }
            ActiveCompletion::Command(result) => {
                let outcome = if command_failure {
                    Ok(CommandOutcome::HandlerFailure {
                        message: error.to_string(),
                    })
                } else {
                    Err(error)
                };
                let _ = result.send(outcome);
            }
            ActiveCompletion::Tree {
                result, completed, ..
            } => {
                completed.store(true, Ordering::Release);
                let _ = result.send(Err(error));
            }
            ActiveCompletion::Call { result, completed } => {
                completed.store(true, Ordering::Release);
                let _ = result.send(Err(error));
            }
            ActiveCompletion::Detached(_) => {}
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn hold_external_utf16(
        &self,
        global_name: &str,
        text: Arc<[u16]>,
    ) -> Result<(), RuntimeError> {
        if self.status.load(Ordering::Acquire) != ACTIVE {
            return Err(self.status_error());
        }
        let data = self.data.lock().unwrap();
        let data = data.as_ref().ok_or_else(RuntimeError::disposed)?;
        let mut locker = data.isolate.lock();
        let scope = pin!(v8::HandleScope::new(&mut *locker));
        let mut scope = scope.init();
        let context = v8::Local::new(&scope, &data.context);
        let scope = &mut v8::ContextScope::new(&mut scope, context);
        let name = v8::String::new(scope, global_name).ok_or_else(RuntimeError::disposed)?;
        let length = text.len();
        let buffer = Arc::into_raw(text).cast::<u16>() as *mut u16;
        // SAFETY: one Arc strong reference was converted into `buffer`; the
        // destructor rebuilds the same slice pointer using V8's retained length.
        let value = unsafe {
            v8::String::new_external_twobyte_raw(scope, buffer, length, drop_external_utf16)
        }
        .ok_or_else(RuntimeError::disposed)?;
        if context.global(scope).set(scope, name.into(), value.into()) != Some(true) {
            return Err(RuntimeError::fatal(
                RuntimeErrorKind::Exception,
                "could not retain external UTF-16 text in the extension context",
            ));
        }
        Ok(())
    }

    pub fn dispose(&self) {
        self.dispose_with_error(RuntimeError::fatal(
            RuntimeErrorKind::Cancelled,
            "extension JavaScript execution was cancelled",
        ));
    }

    pub(super) fn dispose_with_error(&self, error: RuntimeError) {
        if self.status.load(Ordering::Acquire) == DISPOSED {
            return;
        }
        let _ =
            self.status
                .compare_exchange(ACTIVE, TERMINATED, Ordering::AcqRel, Ordering::Acquire);
        let _ = self.heap_state.isolate.terminate_execution();
        let timers = self
            .local_state
            .timers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        drop(timers);
        let Some(data) = self
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        else {
            return;
        };
        self.status.store(DISPOSED, Ordering::Release);
        {
            let mut locker = data.isolate.lock();
            {
                let scope = pin!(v8::HandleScope::new(&mut *locker));
                let mut scope = scope.init();
                let context = v8::Local::new(&scope, &data.context);
                let scope = &mut v8::ContextScope::new(&mut scope, context);
                let pending = std::mem::take(&mut *self.local_state.pending.lock().unwrap());
                for pending in pending.into_values() {
                    if let Some(resolver) = pending.resolver {
                        let resolver = v8::Local::new(scope, &resolver);
                        let cancelled = boundary_exception(
                            scope,
                            BoundaryError::Host(TransportError::Cancelled),
                        );
                        resolver.reject(scope, cancelled);
                    }
                }
                scope.perform_microtask_checkpoint();
            }
            self.fail_all_active(error);
            locker.remove_near_heap_limit_callback(near_heap_limit_callback, 0);
            self.local_state
                .rejections
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
            self.local_state.callbacks.lock().unwrap().clear();
            clear_module_registry(&self.local_state);
            let _ = locker.remove_slot::<Arc<RuntimeLocalState>>();
            drop(data.context);
        }
        drop(data.isolate);
    }

    fn execution_error(
        &self,
        try_catch: &mut v8::PinnedRef<'_, v8::TryCatch<v8::HandleScope>>,
        source_name: &str,
    ) -> RuntimeError {
        if try_catch.has_terminated() || self.status.load(Ordering::Acquire) != ACTIVE {
            self.status_error()
        } else {
            runtime_error_from_try_catch(try_catch, RuntimeErrorKind::Exception, source_name)
        }
    }

    fn status_error(&self) -> RuntimeError {
        match self.status.load(Ordering::Acquire) {
            HEAP_LIMIT_EXCEEDED => RuntimeError::fatal(
                RuntimeErrorKind::HeapLimitExceeded,
                "extension exceeded its JavaScript heap limit",
            ),
            TERMINATED => RuntimeError::fatal(
                RuntimeErrorKind::Terminated,
                "extension JavaScript execution was terminated",
            ),
            _ => RuntimeError::disposed(),
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn install_test_barrier(&self, barrier: Arc<TestGate>) {
        *self.local_state.barrier.lock().unwrap() = Some(barrier);
        let data = self.data.lock().unwrap();
        let data = data.as_ref().unwrap();
        let mut locker = data.isolate.lock();
        let scope = pin!(v8::HandleScope::new(&mut *locker));
        let mut scope = scope.init();
        let context = v8::Local::new(&scope, &data.context);
        let scope = &mut v8::ContextScope::new(&mut scope, context);
        let name = v8::String::new(scope, "__knotTestBarrier").unwrap();
        let function = v8::Function::new(scope, test_barrier_callback).unwrap();
        context
            .global(scope)
            .set(scope, name.into(), function.into());
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn install_test_entered_signal(&self, entered: mpsc::Sender<()>) {
        *self.local_state.entered.lock().unwrap() = Some(entered);
        let data = self.data.lock().unwrap();
        let data = data.as_ref().unwrap();
        let mut locker = data.isolate.lock();
        let scope = pin!(v8::HandleScope::new(&mut *locker));
        let mut scope = scope.init();
        let context = v8::Local::new(&scope, &data.context);
        let scope = &mut v8::ContextScope::new(&mut scope, context);
        let name = v8::String::new(scope, "__knotTestEntered").unwrap();
        let function = v8::Function::new(scope, test_entered_callback).unwrap();
        context
            .global(scope)
            .set(scope, name.into(), function.into());
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_test_host_error(&self, error: HostFailure) {
        *self.local_state.host_error.lock().unwrap() = Some(error);
    }
}

impl Drop for RuntimeCapsule {
    fn drop(&mut self) {
        self.dispose();
    }
}

fn tree_failure(request: &TreeChildrenRequest, error: &RuntimeError) -> TreeChildrenResponse {
    TreeChildrenResponse {
        registration: request.registration,
        window: request.window.clone(),
        view: request.view,
        parent_id: request.parent_id.clone(),
        generation: request.generation,
        result: Err(TreeProviderError {
            message: error.to_string(),
        }),
    }
}

pub(super) fn fatal_or_completed(error: &RuntimeError) -> TurnOutcome {
    if error.is_fatal() {
        TurnOutcome::Fatal(Failure::new(error.to_string()))
    } else {
        TurnOutcome::idle()
    }
}

pub(super) fn execute_engine_turn(
    turn: Turn<EngineTurn>,
    finalizer: &LifecycleFinalizer,
) -> TurnOutcome {
    let key = turn.key;
    let task_root = turn.root;
    let work = turn.work;
    let capsule = Arc::clone(&work.capsule);
    // Work that starts an invocation always carries its root.
    let root = || task_root.expect("an invocation's first task carries its root");
    let starts_invocation = !matches!(
        work.work,
        EngineWork::HostResponse(_)
            | EngineWork::CancelCommand(_)
            | EngineWork::CancelProvider
            | EngineWork::Termination
    );
    if matches!(work.work, EngineWork::PackageModule { .. }) {
        *capsule.startup_root.lock().unwrap() = task_root;
    }
    let started = Instant::now();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match work.work {
        #[cfg(any(test, feature = "test-support"))]
        EngineWork::Script {
            source_name,
            source,
            result,
        } => capsule.start_script_turn(root(), source_name, source, result),
        #[cfg(any(test, feature = "test-support"))]
        EngineWork::FixtureModule {
            specifier,
            source,
            result,
        } => capsule.start_module_turn(root(), specifier, source, result),
        EngineWork::PackageModule { entry, result } => {
            capsule.start_package_turn(root(), entry, result)
        }
        EngineWork::HostResponse(response) => capsule.resume_response_turn(task_root, response),
        EngineWork::BufferChange {
            subscription,
            change,
            enqueued_at,
        } => capsule.start_notification_turn(
            root(),
            subscription.value(),
            ListenerNotification::Buffer(change),
            enqueued_at,
        ),
        EngineWork::ApplicationChange {
            subscription,
            event,
        } => capsule.start_notification_turn(
            root(),
            subscription.value(),
            ListenerNotification::Application(Box::new(event)),
            Instant::now(),
        ),
        EngineWork::EditorEvent { handle, event } => capsule.start_notification_turn(
            root(),
            handle,
            ListenerNotification::Editor(event),
            Instant::now(),
        ),
        EngineWork::Timer(expiry) => {
            let current = capsule
                .local_state
                .timers
                .lock()
                .unwrap()
                .as_mut()
                .is_some_and(|timers| timers.take(expiry));
            if current {
                capsule.start_notification_turn(
                    root(),
                    expiry.id,
                    ListenerNotification::Timer,
                    Instant::now(),
                )
            } else {
                TurnOutcome::idle()
            }
        }
        EngineWork::SettingChange { subscription, key } => capsule.start_notification_turn(
            root(),
            subscription.value(),
            ListenerNotification::Setting(key),
            Instant::now(),
        ),
        EngineWork::WatchChange { subscription, uris } => capsule.start_notification_turn(
            root(),
            subscription.value(),
            ListenerNotification::Watch(uris),
            Instant::now(),
        ),
        EngineWork::CommandInvocation {
            invocation,
            active_buffer,
            result,
        } => capsule.start_command_turn(root(), invocation, active_buffer, result),
        EngineWork::CancelCommand(invocation) => capsule.cancel_command_turn(invocation),
        EngineWork::Provider(call) => capsule.start_provider_turn(root(), call),
        EngineWork::CancelProvider => capsule.cancel_provider_turn(root()),
        EngineWork::Termination => capsule.finish_termination_turn(),
        #[cfg(any(test, feature = "test-support"))]
        EngineWork::Panic { result } => {
            let _result = result;
            panic!("injected extension worker panic")
        }
    }))
    .unwrap_or_else(|_| {
        TurnOutcome::Fatal(Failure::new(
            "extension engine panicked while executing a turn",
        ))
    });
    let mut outcome = outcome;
    if let (TurnOutcome::Completed { settled }, true, Some(task_root)) =
        (&mut outcome, starts_invocation, task_root)
        && !capsule
            .local_state
            .active
            .lock()
            .unwrap()
            .contains_key(&task_root)
        && !settled.contains(&task_root)
    {
        // The invocation never reached an `await`: its result went out
        // during the turn, or it was dropped before running.
        settled.push(task_root);
    }
    if matches!(outcome, TurnOutcome::Fatal(_)) {
        capsule
            .local_state
            .keybinding_mutations
            .lock()
            .unwrap()
            .clear();
        *capsule.local_state.keybinding_batch_pending.lock().unwrap() = None;
        capsule
            .local_state
            .deferred_responses
            .lock()
            .unwrap()
            .clear();
    } else {
        let settled_startup = match &outcome {
            TurnOutcome::Completed { settled } => {
                let mut startup = capsule.startup_root.lock().unwrap();
                settled
                    .iter()
                    .any(|root| startup.take_if(|startup| startup == root).is_some())
            }
            TurnOutcome::Fatal(_) => false,
        };
        if settled_startup || started.elapsed() >= LONG_TURN {
            capsule.release_memory();
        }
    }
    match &outcome {
        TurnOutcome::Fatal(failure) => {
            if capsule.status.load(Ordering::Acquire) != ACTIVE {
                // Every other invocation sees the precise cause.
                capsule.fail_all_active(capsule.status_error());
            }
            finalizer.finalize(
                key,
                LifecycleEnd::Failed(failure.clone()),
                RuntimeError::fatal(
                    RuntimeErrorKind::LifecycleFailed,
                    failure.message().to_owned(),
                ),
                false,
            );
            outcome
        }
        _ => outcome,
    }
}

pub(super) extern "C" fn near_heap_limit_callback(
    data: *mut c_void,
    current_heap_limit: usize,
    _initial_heap_limit: usize,
) -> usize {
    // SAFETY: RuntimeCapsule keeps this boxed state alive until after the
    // callback is removed and the isolate is disposed.
    let state = unsafe { &*(data.cast::<HeapLimitState>()) };
    state.status.store(HEAP_LIMIT_EXCEEDED, Ordering::Release);
    state.isolate.terminate_execution();
    current_heap_limit.saturating_mul(2)
}

pub(super) extern "C" fn promise_reject_callback(message: v8::PromiseRejectMessage) {
    v8::callback_scope!(unsafe scope, &message);
    let Some(state) = scope.get_slot::<Arc<RuntimeLocalState>>().cloned() else {
        return;
    };
    match message.get_event() {
        v8::PromiseRejectEvent::PromiseRejectWithNoHandler => {
            let promise = v8::Global::new(scope, message.get_promise());
            let value = message
                .get_value()
                .unwrap_or_else(|| v8::undefined(scope).into());
            let report = report_from_value(scope, value, promise);
            state
                .rejections
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(report);
        }
        v8::PromiseRejectEvent::PromiseHandlerAddedAfterReject => {
            let promise = message.get_promise();
            let mut rejections = state
                .rejections
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(index) = rejections.iter().position(|report| {
                let retained = v8::Local::new(scope, &report.promise);
                retained == promise
            }) {
                rejections.remove(index);
            }
        }
        v8::PromiseRejectEvent::PromiseRejectAfterResolved
        | v8::PromiseRejectEvent::PromiseResolveAfterResolved => {}
    }
}

pub(super) fn report_from_value(
    scope: &mut v8::PinScope,
    value: v8::Local<v8::Value>,
    promise: v8::Global<v8::Promise>,
) -> RejectionReport {
    let message = v8::Exception::create_message(scope, value);
    let source = message.get_script_resource_name(scope).and_then(|value| {
        value
            .to_string(scope)
            .map(|value| value.to_rust_string_lossy(scope))
    });
    RejectionReport {
        promise,
        message: message.get(scope).to_rust_string_lossy(scope),
        source,
        line: message
            .get_line_number(scope)
            .and_then(|line| u32::try_from(line).ok()),
        column: one_based_coordinate(message.get_start_column()),
        stack: None,
    }
}

pub(super) fn runtime_error_from_try_catch(
    try_catch: &mut v8::PinnedRef<'_, v8::TryCatch<v8::HandleScope>>,
    kind: RuntimeErrorKind,
    fallback_source: &str,
) -> RuntimeError {
    let message_text = try_catch
        .exception()
        .map(|value| value.to_rust_string_lossy(try_catch))
        .unwrap_or_else(|| "JavaScript execution failed".to_owned());
    let message = try_catch.message();
    let source = message
        .and_then(|message| message.get_script_resource_name(try_catch))
        .and_then(|value| value.to_string(try_catch))
        .map(|value| value.to_rust_string_lossy(try_catch))
        .or_else(|| Some(fallback_source.to_owned()));
    let line = message.and_then(|message| {
        message
            .get_line_number(try_catch)
            .and_then(|line| u32::try_from(line).ok())
    });
    let column = message.and_then(|message| one_based_coordinate(message.get_start_column()));
    let stack = try_catch
        .stack_trace()
        .and_then(|value| value.to_string(try_catch))
        .map(|value| value.to_rust_string_lossy(try_catch));
    RuntimeError {
        kind,
        message: message_text.into(),
        source: source.map(Into::into),
        line,
        column,
        stack: stack.map(Into::into),
    }
}

/// The `stack` text of a rejection reason that is an error object.
fn rejection_stack(
    scope: &mut v8::PinnedRef<'_, v8::HandleScope>,
    value: v8::Local<v8::Value>,
) -> Option<Arc<str>> {
    let object = v8::Local::<v8::Object>::try_from(value).ok()?;
    let key = v8::String::new(scope, "stack")?;
    let stack = object.get(scope, key.into())?;
    stack
        .is_string()
        .then(|| stack.to_rust_string_lossy(scope).into())
}

pub(super) fn one_based_coordinate(value: usize) -> Option<u32> {
    u32::try_from(value).ok()?.checked_add(1)
}

pub(super) unsafe extern "C" fn drop_external_utf16(buffer: *mut u16, length: usize) {
    let slice = ptr::slice_from_raw_parts(buffer.cast_const(), length);
    // SAFETY: this exactly rebuilds the Arc slice converted in
    // `hold_external_utf16`, consuming its one transferred strong reference.
    drop(unsafe { Arc::<[u16]>::from_raw(slice) });
}

#[cfg(any(test, feature = "test-support"))]
pub(super) fn test_barrier_callback(
    scope: &mut v8::PinnedRef<'_, v8::HandleScope>,
    _arguments: v8::FunctionCallbackArguments,
    mut result: v8::ReturnValue,
) {
    let state = scope.get_slot::<Arc<RuntimeLocalState>>().cloned();
    let barrier = state
        .as_ref()
        .and_then(|state| state.barrier.lock().ok()?.clone());
    if let Some(barrier) = barrier {
        barrier.wait();
    }
    result.set(v8::undefined(scope).into());
}

#[cfg(any(test, feature = "test-support"))]
pub(super) fn test_entered_callback(
    scope: &mut v8::PinnedRef<'_, v8::HandleScope>,
    _arguments: v8::FunctionCallbackArguments,
    mut result: v8::ReturnValue,
) {
    let entered = scope
        .get_slot::<Arc<RuntimeLocalState>>()
        .and_then(|state| state.entered.lock().ok()?.clone());
    if let Some(entered) = entered {
        let _ = entered.send(());
    }
    result.set(v8::undefined(scope).into());
}
