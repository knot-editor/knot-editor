//! Module-graph compilation, resolution, and rollback inside an isolate.

use super::*;

#[cfg(any(test, feature = "test-support"))]
pub(super) fn validate_root_module_specifier(specifier: &str) -> Result<String, RuntimeError> {
    let parsed = Url::parse(specifier).map_err(|_| RuntimeError {
        kind: RuntimeErrorKind::InvalidModuleSpecifier,
        message: format!("invalid fixture module specifier: {specifier}").into(),
        source: Some(specifier.into()),
        line: None,
        column: None,
        stack: None,
    })?;
    let canonical = parsed.to_string();
    if [
        PRIVATE_BOOTSTRAP_SPECIFIER,
        PUBLIC_FACADE_SPECIFIER,
        CONTRACT_SPECIFIER,
    ]
    .contains(&canonical.as_str())
    {
        return Err(RuntimeError {
            kind: RuntimeErrorKind::InvalidModuleSpecifier,
            message: format!("fixture module specifier is reserved: {canonical}").into(),
            source: Some(canonical.into()),
            line: None,
            column: None,
            stack: None,
        });
    }
    Ok(canonical)
}

pub(super) fn evaluate_fixture_module(
    scope: &mut v8::PinScope<'_, '_>,
    state: &Arc<RuntimeLocalState>,
    specifier: &str,
    source: Option<Arc<str>>,
    internal: bool,
) -> Result<String, RuntimeError> {
    if !internal && specifier == PRIVATE_BOOTSTRAP_SPECIFIER {
        return Err(module_resolution_error(
            specifier,
            "Knot private bootstrap bindings are not importable by extensions",
        ));
    }
    if let Some(source) = source {
        let mut modules = state.modules.lock().unwrap();
        if let Some(existing) = modules.sources.get(specifier) {
            if existing.as_ref() != source.as_ref() {
                return Err(module_resolution_error(
                    specifier,
                    "fixture module source cannot change after registration",
                ));
            }
        } else {
            modules.sources.insert(specifier.to_owned(), source);
        }
    }

    v8::tc_scope!(let try_catch, scope);
    let (module_count, resolution_count) = {
        let modules = state.modules.lock().unwrap();
        (modules.modules.len(), modules.resolutions.len())
    };
    let compiled = {
        let mut modules = state.modules.lock().unwrap();
        compile_module_graph(try_catch, &mut modules, specifier, None, false)
    };
    let compiled = match compiled {
        Ok(compiled) => compiled,
        Err(failure) => {
            let error = failure.report(try_catch, specifier);
            rollback_module_graph(state, module_count, resolution_count);
            return Err(error);
        }
    };
    let module = v8::Local::new(try_catch, &compiled);
    match module.instantiate_module(try_catch, resolve_module_callback) {
        Some(true) => {}
        Some(false) | None => {
            let error = runtime_error_from_try_catch(
                try_catch,
                RuntimeErrorKind::ModuleResolution,
                specifier,
            );
            rollback_module_graph(state, module_count, resolution_count);
            return Err(error);
        }
    }
    let Some(value) = module.evaluate(try_catch) else {
        return Err(runtime_error_from_try_catch(
            try_catch,
            RuntimeErrorKind::Exception,
            specifier,
        ));
    };
    let Ok(promise) = v8::Local::<v8::Promise>::try_from(value) else {
        return Err(RuntimeError::fatal(
            RuntimeErrorKind::Engine,
            "V8 module evaluation did not return a promise",
        ));
    };
    promise.mark_as_handled();
    try_catch.perform_microtask_checkpoint();
    match promise.state() {
        v8::PromiseState::Fulfilled => {
            let value = promise.result(try_catch).to_rust_string_lossy(try_catch);
            let mut rejections = state.rejections.lock().unwrap();
            if let Some(rejection) = rejections.first() {
                let error = RuntimeError {
                    kind: RuntimeErrorKind::Rejection,
                    message: rejection.message.clone().into(),
                    source: rejection.source.clone().map(Into::into),
                    line: rejection.line,
                    column: rejection.column,
                    stack: rejection.stack.clone().map(Into::into),
                };
                rejections.clear();
                Err(error)
            } else {
                Ok(value)
            }
        }
        v8::PromiseState::Rejected => {
            let rejection = promise.result(try_catch);
            let message = v8::Exception::create_message(try_catch, rejection);
            let error = RuntimeError {
                kind: RuntimeErrorKind::Rejection,
                message: message
                    .get(try_catch)
                    .to_rust_string_lossy(try_catch)
                    .into(),
                source: message
                    .get_script_resource_name(try_catch)
                    .and_then(|value| value.to_string(try_catch))
                    .map(|value| value.to_rust_string_lossy(try_catch).into())
                    .or_else(|| Some(specifier.into())),
                line: message
                    .get_line_number(try_catch)
                    .and_then(|line| u32::try_from(line).ok()),
                column: one_based_coordinate(message.get_start_column()),
                stack: None,
            };
            state.rejections.lock().unwrap().clear();
            Err(error)
        }
        v8::PromiseState::Pending => Err(RuntimeError {
            kind: RuntimeErrorKind::Rejection,
            message: "fixture module evaluation remained pending without host work".into(),
            source: Some(specifier.into()),
            line: None,
            column: None,
            stack: None,
        }),
    }
}

pub(super) fn rollback_module_graph(state: &RuntimeLocalState, modules: usize, resolutions: usize) {
    let mut registry = state.modules.lock().unwrap();
    registry.resolutions.truncate(resolutions);
    registry.modules.truncate(modules);
}

pub(super) fn clear_module_registry(state: &RuntimeLocalState) {
    let mut registry = state
        .modules
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    registry.resolutions.clear();
    registry.modules.clear();
    registry.sources.clear();
}

/// Check syntax and static imports without evaluating user code. The caller
/// supplies an immutable graph and runs this away from the foreground thread.
pub fn validate_module_graph_entry(graph: &ModuleGraph, entry: &str) -> Result<(), RuntimeError> {
    let entry = graph.entry_url(entry).ok_or_else(|| {
        RuntimeError::fatal(
            RuntimeErrorKind::InvalidModuleSpecifier,
            format!("module graph entry is unavailable: {entry}"),
        )
    })?;
    initialize();
    let mut isolate = v8::Isolate::new(Default::default());
    let scope = pin!(v8::HandleScope::new(&mut isolate));
    let mut scope = scope.init();
    let context = v8::Context::new(&scope, Default::default());
    let scope = &mut v8::ContextScope::new(&mut scope, context);
    v8::tc_scope!(let try_catch, scope);
    let mut modules = ModuleRegistry::new(Some(graph));
    modules.validating = true;
    let blocked = blocked_post_init_import(&modules, entry);
    compile_module_graph(try_catch, &mut modules, entry, blocked.as_deref(), false)
        .map(|_| ())
        .map_err(|failure| failure.report(try_catch, entry))
}

pub(super) fn blocked_post_init_import(registry: &ModuleRegistry, entry: &str) -> Option<Arc<str>> {
    (registry
        .entries
        .get(ConfigPhase::PreInit.entry())
        .map(AsRef::as_ref)
        == Some(entry))
    .then(|| registry.entries.get(ConfigPhase::PostInit.entry()).cloned())
    .flatten()
}

/// Compiles the package's own contract module before anything else, so every
/// module it reaches is checked against the contract import rules.
pub(super) fn compile_package_contract(
    scope: &mut v8::PinScope<'_, '_>,
    registry: &mut ModuleRegistry,
) -> Result<(), ModuleCompileFailure> {
    match registry
        .package
        .as_ref()
        .and_then(|package| package.contract.clone())
    {
        Some(contract) => compile_module_graph(scope, registry, &contract, None, true).map(drop),
        None => Ok(()),
    }
}

/// Compiles `specifier` and the modules it imports. A module in a contract
/// graph, which a required package's modules always are, may import only
/// `"knot/contract"` and its package's modules other than the main one.
pub(super) fn compile_module_graph(
    scope: &mut v8::PinScope<'_, '_>,
    registry: &mut ModuleRegistry,
    specifier: &str,
    blocked_import: Option<&str>,
    contract: bool,
) -> Result<v8::Global<v8::Module>, ModuleCompileFailure> {
    if let Some(record) = registry
        .modules
        .iter()
        .find(|record| record.specifier == specifier)
    {
        return Ok(record.module.clone());
    }
    let source = if registry.validating && specifier.starts_with(PACKAGE_STAND_IN) {
        Some(Arc::from("export {};"))
    } else {
        registry.sources.get(specifier).cloned()
    };
    let Some(source) = source else {
        throw_module_error(
            scope,
            &if registry.package_root.is_some() {
                format!("Knot package module not found: {specifier}")
            } else {
                format!("Knot fixture module not found: {specifier}")
            },
        );
        return Err(RuntimeErrorKind::ModuleResolution.into());
    };
    let source_text = v8::String::new(scope, &source).ok_or(RuntimeErrorKind::Engine)?;
    let resource_name = v8::String::new(scope, specifier).ok_or(RuntimeErrorKind::Engine)?;
    let origin = v8::ScriptOrigin::new(
        scope,
        resource_name.into(),
        0,
        0,
        false,
        0,
        None,
        false,
        false,
        true,
        None,
    );
    let mut source = v8::script_compiler::Source::new(source_text, Some(&origin));
    let module = v8::script_compiler::compile_module(scope, &mut source)
        .ok_or(RuntimeErrorKind::Compilation)?;
    let global = v8::Global::new(scope, module);
    registry.modules.push(CompiledModule {
        specifier: specifier.to_owned(),
        module: global.clone(),
    });

    let requests = module.get_module_requests();
    for index in 0..requests.length() {
        let request = requests.get(scope, index).ok_or(RuntimeErrorKind::Engine)?;
        let request = v8::Local::<v8::ModuleRequest>::try_from(request)
            .map_err(|_| RuntimeErrorKind::Engine)?;
        let location = module.source_offset_to_location(request.get_source_offset());
        let request = request.get_specifier().to_rust_string_lossy(scope);
        let resolved = match resolve_module_specifier(&request, specifier, registry, contract) {
            Ok(resolved) => resolved,
            Err(message) => {
                throw_module_error(scope, &message);
                return Err(ModuleCompileFailure::at_import(
                    RuntimeErrorKind::ModuleResolution,
                    specifier,
                    location,
                ));
            }
        };
        if blocked_import == Some(resolved.as_str()) {
            throw_module_error(scope, "pre-init cannot import post-init.js");
            return Err(ModuleCompileFailure::at_import(
                RuntimeErrorKind::ModuleResolution,
                specifier,
                location,
            ));
        }
        if let Some(owner) = resolved.strip_prefix(OWNED_CONTRACT_PREFIX) {
            let source = owned_contract_source(owner);
            registry
                .sources
                .entry(resolved.clone())
                .or_insert_with(|| source.into());
        }
        let stand_in = registry.validating && resolved.starts_with(PACKAGE_STAND_IN);
        if !stand_in && !registry.sources.contains_key(&resolved) {
            let kind = if registry.package_root.is_some() {
                "package"
            } else {
                "fixture"
            };
            throw_module_error(scope, &format!("Knot {kind} module not found: {resolved}"));
            return Err(ModuleCompileFailure::at_import(
                RuntimeErrorKind::ModuleResolution,
                specifier,
                location,
            ));
        }
        registry.resolutions.push(ModuleResolution {
            referrer: global.clone(),
            request,
            resolved: resolved.clone(),
        });
        let contract = contract || registry.dependency_at(&resolved).is_some();
        compile_module_graph(scope, registry, &resolved, blocked_import, contract)?;
    }
    Ok(global)
}

/// The `"knot/contract"` module of one owner: the shared builder, with
/// contracts qualified by the owner.
fn owned_contract_source(owner: &str) -> String {
    format!(
        "import {{ contractModule }} from {bootstrap};\nexport const {{ s, defineContract }} = contractModule({owner});\n",
        bootstrap = serde_json::Value::from(PRIVATE_BOOTSTRAP_SPECIFIER),
        owner = serde_json::Value::from(owner),
    )
}

/// Who owns the contracts the module at `referrer` defines: the required
/// package whose root holds it, else the graph's own package.
fn contract_owner(registry: &ModuleRegistry, referrer: &str) -> String {
    registry
        .dependency_at(referrer)
        .or(registry.package.as_ref())
        .map(|package| package.package.to_string())
        .or_else(|| registry.package_root.as_deref().map(str::to_owned))
        .unwrap_or_else(|| "fixture".into())
}

/// The prefix of the empty module a package import resolves to while a graph is
/// only checked.
const PACKAGE_STAND_IN: &str = "knot-validation:";

const CONTRACT_IMPORTS: &str =
    "a contract module may import only knot/contract and its own package's modules";

pub(super) fn resolve_module_specifier(
    request: &str,
    referrer: &str,
    registry: &ModuleRegistry,
    contract: bool,
) -> Result<String, String> {
    let owned_contract = referrer.starts_with(OWNED_CONTRACT_PREFIX);
    if request == PRIVATE_BOOTSTRAP_SPECIFIER {
        return if referrer == PUBLIC_FACADE_SPECIFIER || owned_contract {
            Ok(request.to_owned())
        } else {
            Err("Knot private bootstrap bindings are not importable by extensions".into())
        };
    }
    if request == CONTRACT_SPECIFIER {
        return Ok(if referrer == PRIVATE_BOOTSTRAP_SPECIFIER {
            request.to_owned()
        } else {
            format!(
                "{OWNED_CONTRACT_PREFIX}{}",
                contract_owner(registry, referrer)
            )
        });
    }
    if request == PUBLIC_FACADE_SPECIFIER {
        return if contract {
            Err(format!("{CONTRACT_IMPORTS}: {request}"))
        } else {
            Ok(request.to_owned())
        };
    }
    let built_in = owned_contract
        || [
            PRIVATE_BOOTSTRAP_SPECIFIER,
            PUBLIC_FACADE_SPECIFIER,
            CONTRACT_SPECIFIER,
        ]
        .contains(&referrer);
    if let Some(package_root) = registry.package_root.as_deref() {
        if built_in || (!request.starts_with("./") && !request.starts_with("../")) {
            if contract {
                return Err(format!("{CONTRACT_IMPORTS}: {request}"));
            }
            if let Some(contract) = registry
                .dependencies
                .iter()
                .filter(|_| !built_in)
                .find(|dependency| &*dependency.package == request)
                .and_then(|dependency| dependency.contract.as_deref())
            {
                return Ok(contract.to_owned());
            }
            if registry.validating && registry.package.is_none() {
                // Personal config may import any loaded package, which is
                // known only once packages load: check the import then.
                return Ok(format!("{PACKAGE_STAND_IN}{request}"));
            }
            return Err(format!(
                "extension import must be relative, knot, knot/contract, or a required package: {request}"
            ));
        }
        let dependency = registry.dependency_at(referrer);
        let root = dependency.map_or(package_root, |dependency| &*dependency.root);
        let resolved = Url::parse(referrer)
            .and_then(|base| base.join(request))
            .map_err(|_| format!("invalid extension import: {request}"))?;
        if !resolved.as_str().starts_with(root)
            || resolved.query().is_some()
            || resolved.fragment().is_some()
        {
            return Err(format!("extension import escapes package: {request}"));
        }
        let main = dependency
            .or(registry.package.as_ref())
            .map(|package| &*package.main);
        if contract && main == Some(resolved.as_str()) {
            return Err(format!(
                "a contract module cannot import its package's main module: {request}"
            ));
        }
        return Ok(resolved.to_string());
    }
    if let Ok(absolute) = Url::parse(request) {
        return Ok(absolute.to_string());
    }
    if !request.starts_with("./") && !request.starts_with("../") && !request.starts_with('/') {
        return Err(format!("invalid fixture module specifier: {request}"));
    }
    Url::parse(referrer)
        .and_then(|base| base.join(request))
        .map(|url| url.to_string())
        .map_err(|_| format!("invalid fixture module specifier: {request}"))
}

pub(super) fn module_resolution_error(source: &str, message: &str) -> RuntimeError {
    RuntimeError {
        kind: RuntimeErrorKind::ModuleResolution,
        message: message.into(),
        source: Some(source.into()),
        line: None,
        column: None,
        stack: None,
    }
}

pub(super) fn throw_module_error(scope: &mut v8::PinScope<'_, '_>, message: &str) {
    if let Some(message) = v8::String::new(scope, message) {
        let exception = v8::Exception::type_error(scope, message);
        scope.throw_exception(exception);
    }
}

pub(super) fn reject_dynamic_import<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    _options: v8::Local<'s, v8::Data>,
    _resource_name: v8::Local<'s, v8::Value>,
    _specifier: v8::Local<'s, v8::String>,
    _attributes: v8::Local<'s, v8::FixedArray>,
) -> Option<v8::Local<'s, v8::Promise>> {
    let resolver = v8::PromiseResolver::new(scope)?;
    let message = v8::String::new(scope, "dynamic imports are unavailable to extensions")?;
    let error = v8::Exception::type_error(scope, message);
    resolver.reject(scope, error);
    Some(resolver.get_promise(scope))
}

pub(super) fn resolve_module_callback<'s>(
    context: v8::Local<'s, v8::Context>,
    specifier: v8::Local<'s, v8::String>,
    _import_attributes: v8::Local<'s, v8::FixedArray>,
    referrer: v8::Local<'s, v8::Module>,
) -> Option<v8::Local<'s, v8::Module>> {
    v8::callback_scope!(unsafe scope, context);
    let state = scope.get_slot::<Arc<RuntimeLocalState>>()?.clone();
    let request = specifier.to_rust_string_lossy(scope);
    let registry = state.modules.lock().ok()?;
    let resolution = registry.resolutions.iter().find(|resolution| {
        resolution.request == request && v8::Local::new(scope, &resolution.referrer) == referrer
    });
    let Some(resolution) = resolution else {
        throw_module_error(scope, &format!("unresolved module import: {request}"));
        return None;
    };
    registry
        .modules
        .iter()
        .find(|record| record.specifier == resolution.resolved)
        .map(|record| v8::Local::new(scope, &record.module))
}
