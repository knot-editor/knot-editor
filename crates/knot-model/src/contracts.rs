//! Contract schemas: the descriptors `knot/contract` builds, compiled once
//! and walked over parsed JSON values. A call checks its argument and result
//! against the consumer's and the provider's schemas, and a failure names the
//! method, the path of the offending value, and whose schema rejected it.

use std::{collections::BTreeMap, fmt, sync::Arc};

use serde_json::{Map, Value};

/// The largest integer JavaScript represents exactly, as elsewhere on the
/// extension boundary.
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

#[derive(Clone, Debug, PartialEq)]
enum Schema {
    String,
    Int,
    Number,
    Boolean,
    Null,
    Json,
    /// No value: a method without an argument or result, carried as null.
    Void,
    Literal(Value),
    Enum(Vec<String>),
    Array(Box<Schema>),
    Tuple(Vec<Schema>),
    Record(Box<Schema>),
    Object(BTreeMap<String, Field>),
    Union(Vec<Schema>),
}

#[derive(Clone, Debug, PartialEq)]
struct Field {
    schema: Schema,
    optional: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct Method {
    argument: Schema,
    result: Schema,
}

/// One side's compiled contract: its methods and their argument and result
/// schemas. Two schemas compare equal when their descriptors are
/// structurally equal.
#[derive(Clone, Debug, PartialEq)]
pub struct ContractSchema {
    methods: BTreeMap<String, Method>,
}

/// A descriptor the compiler rejects, at a dotted path into the descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DescriptorError {
    pub path: String,
    pub message: String,
}

impl fmt::Display for DescriptorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(formatter, "invalid contract descriptor: {}", self.message)
        } else {
            write!(
                formatter,
                "invalid contract descriptor at {}: {}",
                self.path, self.message
            )
        }
    }
}

impl std::error::Error for DescriptorError {}

impl ContractSchema {
    /// Compiles the `methods` object of a contract value.
    pub fn compile(methods: &Value) -> Result<Self, DescriptorError> {
        let mut path = Vec::new();
        let methods = object(methods, &path, "methods")?;
        if methods.is_empty() {
            return Err(error(&path, "a contract needs at least one method"));
        }
        let mut compiled = BTreeMap::new();
        for (name, method) in methods {
            path.push(name.clone());
            let node = object(method, &path, "a method")?;
            expect_kind(node, &path, "fn", &["argument", "result"])?;
            let argument = compile_part(node, "argument", &mut path)?;
            let result = compile_part(node, "result", &mut path)?;
            path.pop();
            compiled.insert(name.clone(), Method { argument, result });
        }
        Ok(Self { methods: compiled })
    }

    /// Compiles a descriptor serialized as JSON text.
    pub fn parse(methods: &str) -> Result<Self, DescriptorError> {
        let value = serde_json::from_str(methods).map_err(|error| DescriptorError {
            path: String::new(),
            message: error.to_string(),
        })?;
        Self::compile(&value)
    }

    pub fn methods(&self) -> impl Iterator<Item = &str> {
        self.methods.keys().map(String::as_str)
    }
}

/// Whose schema rejected a value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Side {
    Consumer,
    Provider,
}

impl fmt::Display for Side {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Consumer => "consumer",
            Self::Provider => "provider",
        })
    }
}

/// A call argument or result one side's schema rejects. `path` starts at
/// `argument` or `result`, such as `result[2].label`; it is empty when that
/// side's contract has no such method.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractViolation {
    pub method: String,
    pub side: Side,
    pub path: String,
    pub message: String,
}

impl fmt::Display for ContractViolation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(formatter, "{}: {}", self.method, self.message)
        } else {
            write!(
                formatter,
                "{}: {}: {} (the {}'s schema)",
                self.method, self.path, self.message, self.side
            )
        }
    }
}

impl std::error::Error for ContractViolation {}

/// The schemas one consumer's calls to one provider registration validate
/// against. The consumer's schema is kept only when it differs from the
/// provider's, so a matching consumer checks each value once.
#[derive(Clone, Debug)]
pub struct ContractBinding {
    provider: Arc<ContractSchema>,
    consumer: Option<Arc<ContractSchema>>,
}

impl ContractBinding {
    /// A consumer of `None` trusts the provider's schema, as native callers do.
    pub fn new(provider: Arc<ContractSchema>, consumer: Option<Arc<ContractSchema>>) -> Self {
        let consumer = consumer
            .filter(|consumer| !Arc::ptr_eq(consumer, &provider) && **consumer != *provider);
        Self { provider, consumer }
    }

    /// True when the consumer's schema differs from the provider's and each
    /// value is checked against both.
    pub fn checks_both(&self) -> bool {
        self.consumer.is_some()
    }

    /// Checks an argument against the consumer's, then the provider's schema.
    pub fn validate_argument(
        &self,
        method: &str,
        argument: &Value,
    ) -> Result<(), ContractViolation> {
        let sides = [
            self.consumer
                .as_deref()
                .map(|schema| (Side::Consumer, schema)),
            Some((Side::Provider, self.provider.as_ref())),
        ];
        for (side, schema) in sides.into_iter().flatten() {
            let method_schema = lookup(schema, side, method)?;
            check(&method_schema.argument, argument, "argument", method, side)?;
        }
        Ok(())
    }

    /// Checks a result against the provider's, then the consumer's schema.
    pub fn validate_result(&self, method: &str, result: &Value) -> Result<(), ContractViolation> {
        let sides = [
            Some((Side::Provider, self.provider.as_ref())),
            self.consumer
                .as_deref()
                .map(|schema| (Side::Consumer, schema)),
        ];
        for (side, schema) in sides.into_iter().flatten() {
            let method_schema = lookup(schema, side, method)?;
            check(&method_schema.result, result, "result", method, side)?;
        }
        Ok(())
    }
}

fn lookup<'a>(
    schema: &'a ContractSchema,
    side: Side,
    method: &str,
) -> Result<&'a Method, ContractViolation> {
    schema.methods.get(method).ok_or_else(|| ContractViolation {
        method: method.to_owned(),
        side,
        path: String::new(),
        message: format!("the {side}'s contract has no such method"),
    })
}

fn check(
    schema: &Schema,
    value: &Value,
    root: &str,
    method: &str,
    side: Side,
) -> Result<(), ContractViolation> {
    let mut path = vec![Segment::Root(root)];
    validate(schema, value, &mut path).map_err(|message| ContractViolation {
        method: method.to_owned(),
        side,
        path: render(&path),
        message,
    })
}

enum Segment<'a> {
    Root(&'a str),
    Index(usize),
    Key(&'a str),
}

fn render(path: &[Segment<'_>]) -> String {
    let mut rendered = String::new();
    for segment in path {
        match segment {
            Segment::Root(root) => rendered.push_str(root),
            Segment::Index(index) => rendered.push_str(&format!("[{index}]")),
            Segment::Key(key) if identifier(key) => {
                rendered.push('.');
                rendered.push_str(key);
            }
            Segment::Key(key) => rendered.push_str(&format!("[{}]", Value::from(*key))),
        }
    }
    rendered
}

fn identifier(key: &str) -> bool {
    let mut characters = key.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_' || first == '$')
        && characters.all(|rest| rest.is_ascii_alphanumeric() || rest == '_' || rest == '$')
}

/// Walks `value`; on failure `path` is left pointing at the offending value.
fn validate<'a>(
    schema: &Schema,
    value: &'a Value,
    path: &mut Vec<Segment<'a>>,
) -> Result<(), String> {
    let mismatch = || {
        Err(format!(
            "expected {}, found {}",
            expected(schema),
            found(value)
        ))
    };
    match (schema, value) {
        (Schema::Json, _)
        | (Schema::String, Value::String(_))
        | (Schema::Number, Value::Number(_))
        | (Schema::Boolean, Value::Bool(_))
        | (Schema::Null | Schema::Void, Value::Null) => Ok(()),
        (Schema::Int, Value::Number(number)) if safe_integer(number) => Ok(()),
        (Schema::Literal(literal), _) if same_scalar(literal, value) => Ok(()),
        (Schema::Enum(values), Value::String(text)) if values.contains(text) => Ok(()),
        (Schema::Array(items), Value::Array(values)) => {
            for (index, item) in values.iter().enumerate() {
                path.push(Segment::Index(index));
                validate(items, item, path)?;
                path.pop();
            }
            Ok(())
        }
        (Schema::Tuple(items), Value::Array(values)) => {
            if items.len() != values.len() {
                return Err(format!(
                    "expected {} items, found {}",
                    items.len(),
                    values.len()
                ));
            }
            for (index, (item, value)) in items.iter().zip(values).enumerate() {
                path.push(Segment::Index(index));
                validate(item, value, path)?;
                path.pop();
            }
            Ok(())
        }
        (Schema::Record(values_schema), Value::Object(values)) => {
            for (key, item) in values {
                path.push(Segment::Key(key));
                validate(values_schema, item, path)?;
                path.pop();
            }
            Ok(())
        }
        (Schema::Object(fields), Value::Object(values)) => {
            if let Some(key) = values.keys().find(|key| !fields.contains_key(*key)) {
                path.push(Segment::Key(key));
                return Err("unexpected field".into());
            }
            for (name, field) in fields {
                match values.get_key_value(name) {
                    Some((key, item)) => {
                        path.push(Segment::Key(key));
                        validate(&field.schema, item, path)?;
                        path.pop();
                    }
                    None if field.optional => {}
                    None => return Err(format!("missing field {}", Value::from(name.as_str()))),
                }
            }
            Ok(())
        }
        (Schema::Union(options), _) => {
            let mut scratch = Vec::new();
            if options
                .iter()
                .any(|option| validate(option, value, &mut scratch).is_ok())
            {
                Ok(())
            } else {
                mismatch()
            }
        }
        _ => mismatch(),
    }
}

fn safe_integer(number: &serde_json::Number) -> bool {
    if let Some(integer) = number.as_i64() {
        return integer.unsigned_abs() <= MAX_SAFE_INTEGER as u64;
    }
    if let Some(integer) = number.as_u64() {
        return integer <= MAX_SAFE_INTEGER as u64;
    }
    number
        .as_f64()
        .is_some_and(|float| float.fract() == 0.0 && float.abs() <= MAX_SAFE_INTEGER)
}

fn same_scalar(literal: &Value, value: &Value) -> bool {
    match (literal, value) {
        (Value::Number(literal), Value::Number(value)) => literal.as_f64() == value.as_f64(),
        _ => literal == value,
    }
}

fn expected(schema: &Schema) -> String {
    match schema {
        Schema::String => "string".into(),
        Schema::Int => "safe integer".into(),
        Schema::Number => "number".into(),
        Schema::Boolean => "boolean".into(),
        Schema::Null => "null".into(),
        Schema::Json => "JSON value".into(),
        Schema::Void => "no value".into(),
        Schema::Literal(value) => value.to_string(),
        Schema::Enum(values) => format!(
            "one of {}",
            values
                .iter()
                .map(|value| Value::from(value.as_str()).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Schema::Array(_) => "array".into(),
        Schema::Tuple(items) => format!("tuple of {}", items.len()),
        Schema::Record(_) | Schema::Object(_) => "object".into(),
        Schema::Union(options) => options
            .iter()
            .map(expected)
            .collect::<Vec<_>>()
            .join(" or "),
    }
}

fn found(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(value) => format!("boolean {value}"),
        Value::Number(value) => format!("number {value}"),
        Value::String(value) => format!("string {}", Value::from(value.as_str())),
        Value::Array(_) => "array".into(),
        Value::Object(_) => "object".into(),
    }
}

fn error(path: &[String], message: impl Into<String>) -> DescriptorError {
    DescriptorError {
        path: path.join("."),
        message: message.into(),
    }
}

fn object<'a>(
    value: &'a Value,
    path: &[String],
    what: &str,
) -> Result<&'a Map<String, Value>, DescriptorError> {
    value
        .as_object()
        .ok_or_else(|| error(path, format!("{what} must be an object")))
}

/// Checks that `node` has `kind` and no keys beyond `kind` and `allowed`.
fn expect_kind(
    node: &Map<String, Value>,
    path: &[String],
    kind: &str,
    allowed: &[&str],
) -> Result<(), DescriptorError> {
    match node.get("kind") {
        Some(Value::String(found)) if found == kind => {}
        Some(Value::String(found)) => {
            return Err(error(
                path,
                format!("expected kind \"{kind}\", found \"{found}\""),
            ));
        }
        _ => return Err(error(path, "kind must be a string")),
    }
    unexpected_keys(node, path, allowed)
}

fn unexpected_keys(
    node: &Map<String, Value>,
    path: &[String],
    allowed: &[&str],
) -> Result<(), DescriptorError> {
    match node
        .keys()
        .find(|key| *key != "kind" && !allowed.contains(&key.as_str()))
    {
        Some(key) => Err(error(path, format!("unexpected key \"{key}\""))),
        None => Ok(()),
    }
}

fn member<'a>(
    node: &'a Map<String, Value>,
    key: &str,
    path: &[String],
) -> Result<&'a Value, DescriptorError> {
    node.get(key)
        .ok_or_else(|| error(path, format!("missing key \"{key}\"")))
}

fn compile_part(
    node: &Map<String, Value>,
    key: &str,
    path: &mut Vec<String>,
) -> Result<Schema, DescriptorError> {
    let value = member(node, key, path)?;
    path.push(key.to_owned());
    let compiled = match value.get("kind") {
        Some(Value::String(kind)) if kind == "void" => {
            unexpected_keys(object(value, path, "a schema")?, path, &[])?;
            Schema::Void
        }
        _ => compile_schema(value, path)?,
    };
    path.pop();
    Ok(compiled)
}

fn compile_schema(value: &Value, path: &mut Vec<String>) -> Result<Schema, DescriptorError> {
    let node = object(value, path, "a schema")?;
    let Some(Value::String(kind)) = node.get("kind") else {
        return Err(error(path, "kind must be a string"));
    };
    let allowed: &[&str] = match kind.as_str() {
        "string" | "int" | "number" | "boolean" | "null" | "json" => &[],
        "literal" => &["value"],
        "enum" => &["values"],
        "array" | "tuple" => &["items"],
        "record" => &["values"],
        "object" => &["fields"],
        "union" => &["options"],
        "void" => return Err(error(path, "void is only a method argument or result")),
        "optional" => return Err(error(path, "optional is only an object field")),
        "fn" => return Err(error(path, "fn is only a contract method")),
        other => return Err(error(path, format!("unknown schema kind \"{other}\""))),
    };
    unexpected_keys(node, path, allowed)?;
    Ok(match kind.as_str() {
        "string" => Schema::String,
        "int" => Schema::Int,
        "number" => Schema::Number,
        "boolean" => Schema::Boolean,
        "null" => Schema::Null,
        "json" => Schema::Json,
        "literal" => {
            let value = member(node, "value", path)?;
            if value.is_array() || value.is_object() {
                return Err(error(
                    path,
                    "a literal must be a string, number, boolean, or null",
                ));
            }
            Schema::Literal(value.clone())
        }
        "enum" => {
            let Some(values) = member(node, "values", path)?.as_array() else {
                return Err(error(path, "values must be an array"));
            };
            let mut strings = Vec::with_capacity(values.len());
            for value in values {
                let Some(value) = value.as_str() else {
                    return Err(error(path, "enum values must be strings"));
                };
                if strings.iter().any(|existing| existing == value) {
                    return Err(error(path, format!("duplicate enum value \"{value}\"")));
                }
                strings.push(value.to_owned());
            }
            if strings.is_empty() {
                return Err(error(path, "an enum needs at least one value"));
            }
            Schema::Enum(strings)
        }
        "array" => Schema::Array(Box::new(compile_child(node, "items", path)?)),
        "record" => Schema::Record(Box::new(compile_child(node, "values", path)?)),
        "tuple" => Schema::Tuple(compile_list(node, "items", path)?),
        "union" => {
            let options = compile_list(node, "options", path)?;
            if options.is_empty() {
                return Err(error(path, "a union needs at least one option"));
            }
            Schema::Union(options)
        }
        "object" => {
            let fields = object(member(node, "fields", path)?, path, "fields")?;
            path.push("fields".into());
            let mut compiled = BTreeMap::new();
            for (name, field) in fields {
                path.push(name.clone());
                let optional = field.get("kind").and_then(Value::as_str) == Some("optional");
                let schema = if optional {
                    let node = object(field, path, "a schema")?;
                    unexpected_keys(node, path, &["schema"])?;
                    compile_child(node, "schema", path)?
                } else {
                    compile_schema(field, path)?
                };
                path.pop();
                compiled.insert(name.clone(), Field { schema, optional });
            }
            path.pop();
            Schema::Object(compiled)
        }
        _ => unreachable!("kinds are matched above"),
    })
}

fn compile_child(
    node: &Map<String, Value>,
    key: &str,
    path: &mut Vec<String>,
) -> Result<Schema, DescriptorError> {
    let value = member(node, key, path)?;
    path.push(key.to_owned());
    let compiled = compile_schema(value, path)?;
    path.pop();
    Ok(compiled)
}

fn compile_list(
    node: &Map<String, Value>,
    key: &str,
    path: &mut Vec<String>,
) -> Result<Vec<Schema>, DescriptorError> {
    let Some(values) = member(node, key, path)?.as_array() else {
        return Err(error(path, format!("{key} must be an array")));
    };
    path.push(key.to_owned());
    let mut compiled = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        path.push(index.to_string());
        compiled.push(compile_schema(value, path)?);
        path.pop();
    }
    path.pop();
    Ok(compiled)
}

#[cfg(test)]
mod tests;
