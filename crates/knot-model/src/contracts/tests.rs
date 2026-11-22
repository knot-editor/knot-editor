use std::sync::Arc;

use serde_json::{Value, json};

use super::*;

fn method(argument: Value, result: Value) -> Value {
    json!({ "kind": "fn", "argument": argument, "result": result })
}

fn compile(methods: Value) -> Arc<ContractSchema> {
    Arc::new(ContractSchema::compile(&methods).unwrap())
}

/// A provider-only binding whose one method `call` takes `argument`.
fn taking(argument: Value) -> ContractBinding {
    ContractBinding::new(
        compile(json!({ "call": method(argument, json!({ "kind": "void" })) })),
        None,
    )
}

fn argument_error(argument: Value, value: Value) -> String {
    taking(argument)
        .validate_argument("call", &value)
        .unwrap_err()
        .to_string()
}

fn accepts(argument: Value, value: Value) -> bool {
    taking(argument).validate_argument("call", &value).is_ok()
}

fn descriptor_error(methods: Value) -> String {
    ContractSchema::compile(&methods).unwrap_err().to_string()
}

#[test]
fn scalar_kinds_accept_their_values_only() {
    let cases = [
        (json!({ "kind": "string" }), json!("text"), json!(1)),
        (json!({ "kind": "int" }), json!(-3), json!(1.5)),
        (json!({ "kind": "number" }), json!(1.5), json!("1.5")),
        (json!({ "kind": "boolean" }), json!(false), json!(null)),
        (json!({ "kind": "null" }), json!(null), json!(false)),
        (
            json!({ "kind": "literal", "value": "a" }),
            json!("a"),
            json!("b"),
        ),
        (
            json!({ "kind": "literal", "value": 2 }),
            json!(2.0),
            json!(3),
        ),
        (
            json!({ "kind": "literal", "value": null }),
            json!(null),
            json!(0),
        ),
        (
            json!({ "kind": "enum", "values": ["a", "b"] }),
            json!("b"),
            json!("c"),
        ),
    ];
    for (schema, good, bad) in cases {
        assert!(
            accepts(schema.clone(), good.clone()),
            "{schema} accepts {good}"
        );
        assert!(
            !accepts(schema.clone(), bad.clone()),
            "{schema} rejects {bad}"
        );
    }
}

#[test]
fn json_accepts_any_value() {
    for value in [json!(null), json!([1, { "a": [true] }]), json!("x")] {
        assert!(accepts(json!({ "kind": "json" }), value));
    }
}

#[test]
fn int_accepts_only_safe_integers() {
    let int = json!({ "kind": "int" });
    for value in [
        json!(9_007_199_254_740_991_i64),
        json!(-9_007_199_254_740_991_i64),
        json!(4.0),
    ] {
        assert!(accepts(int.clone(), value.clone()), "accepts {value}");
    }
    for value in [
        json!(9_007_199_254_740_992_i64),
        json!(-9_007_199_254_740_992_i64),
        json!(u64::MAX),
        json!(1e300),
        json!(0.5),
    ] {
        assert!(!accepts(int.clone(), value.clone()), "rejects {value}");
    }
    assert_eq!(
        argument_error(int, json!(9_007_199_254_740_992_i64)),
        "call: argument: expected safe integer, found number 9007199254740992 (the provider's schema)"
    );
}

#[test]
fn containers_validate_their_items() {
    let array = json!({ "kind": "array", "items": { "kind": "string" } });
    assert!(accepts(array.clone(), json!([])));
    assert!(accepts(array.clone(), json!(["a", "b"])));
    assert_eq!(
        argument_error(array, json!(["a", 2])),
        "call: argument[1]: expected string, found number 2 (the provider's schema)"
    );

    let tuple = json!({ "kind": "tuple", "items": [{ "kind": "string" }, { "kind": "int" }] });
    assert!(accepts(tuple.clone(), json!(["a", 1])));
    assert_eq!(
        argument_error(tuple.clone(), json!(["a"])),
        "call: argument: expected 2 items, found 1 (the provider's schema)"
    );
    assert_eq!(
        argument_error(tuple, json!(["a", "b"])),
        "call: argument[1]: expected safe integer, found string \"b\" (the provider's schema)"
    );

    let record = json!({ "kind": "record", "values": { "kind": "boolean" } });
    assert!(accepts(
        record.clone(),
        json!({ "any": true, "other key": false })
    ));
    assert_eq!(
        argument_error(record, json!({ "other key": 1 })),
        "call: argument[\"other key\"]: expected boolean, found number 1 (the provider's schema)"
    );
}

#[test]
fn objects_are_strict_and_optional_fields_may_be_absent() {
    let object = json!({
        "kind": "object",
        "fields": {
            "label": { "kind": "string" },
            "detail": { "kind": "optional", "schema": { "kind": "string" } },
        },
    });
    assert!(accepts(object.clone(), json!({ "label": "a" })));
    assert!(accepts(
        object.clone(),
        json!({ "label": "a", "detail": "b" })
    ));
    assert_eq!(
        argument_error(object.clone(), json!({ "label": "a", "extra": 1 })),
        "call: argument.extra: unexpected field (the provider's schema)"
    );
    assert_eq!(
        argument_error(object.clone(), json!({ "detail": "b" })),
        "call: argument: missing field \"label\" (the provider's schema)"
    );
    assert_eq!(
        argument_error(object.clone(), json!({ "label": "a", "detail": null })),
        "call: argument.detail: expected string, found null (the provider's schema)"
    );
    assert_eq!(
        argument_error(object, json!(["label"])),
        "call: argument: expected object, found array (the provider's schema)"
    );
}

#[test]
fn unions_accept_any_option() {
    let nullable = json!({
        "kind": "union",
        "options": [{ "kind": "string" }, { "kind": "null" }],
    });
    assert!(accepts(nullable.clone(), json!("a")));
    assert!(accepts(nullable.clone(), json!(null)));
    assert_eq!(
        argument_error(nullable, json!(1)),
        "call: argument: expected string or null, found number 1 (the provider's schema)"
    );
    let tagged = json!({
        "kind": "union",
        "options": [
            { "kind": "literal", "value": "a" },
            { "kind": "enum", "values": ["b", "c"] },
        ],
    });
    assert_eq!(
        argument_error(tagged, json!("d")),
        "call: argument: expected \"a\" or one of \"b\", \"c\", found string \"d\" (the provider's schema)"
    );
}

#[test]
fn void_carries_null_only() {
    let binding = taking(json!({ "kind": "void" }));
    assert!(binding.validate_argument("call", &json!(null)).is_ok());
    assert!(binding.validate_result("call", &json!(null)).is_ok());
    assert_eq!(
        binding
            .validate_result("call", &json!(1))
            .unwrap_err()
            .to_string(),
        "call: result: expected no value, found number 1 (the provider's schema)"
    );
}

#[test]
fn nested_paths_name_the_offending_value() {
    let item = json!({
        "kind": "object",
        "fields": { "label": { "kind": "string" }, "$id": { "kind": "int" } },
    });
    let binding = ContractBinding::new(
        compile(json!({
            "rows": method(
                json!({ "kind": "string" }),
                json!({ "kind": "array", "items": item }),
            ),
        })),
        None,
    );
    let violation = binding
        .validate_result(
            "rows",
            &json!([{ "label": "a", "$id": 1 }, { "label": 2, "$id": 2 }]),
        )
        .unwrap_err();
    assert_eq!(violation.side, Side::Provider);
    assert_eq!(violation.method, "rows");
    assert_eq!(violation.path, "result[1].label");
}

#[test]
fn a_differing_consumer_checks_each_side_in_call_order() {
    let provider = compile(json!({
        "rows": method(json!({ "kind": "string" }), json!({ "kind": "int" })),
    }));
    let consumer = compile(json!({
        "rows": method(json!({ "kind": "json" }), json!({ "kind": "string" })),
        "extra": method(json!({ "kind": "void" }), json!({ "kind": "void" })),
    }));
    let binding = ContractBinding::new(provider, Some(consumer));
    assert!(binding.checks_both());

    let argument = binding.validate_argument("rows", &json!(1)).unwrap_err();
    assert_eq!(
        argument.side,
        Side::Provider,
        "the consumer's json lets it through"
    );
    let result = binding.validate_result("rows", &json!(1)).unwrap_err();
    assert_eq!(
        result.side,
        Side::Consumer,
        "the provider's int lets it through"
    );
    assert_eq!(
        result.to_string(),
        "rows: result: expected string, found number 1 (the consumer's schema)"
    );
    assert_eq!(
        binding
            .validate_argument("extra", &json!(null))
            .unwrap_err()
            .to_string(),
        "extra: the provider's contract has no such method"
    );
    assert_eq!(
        binding
            .validate_argument("missing", &json!(null))
            .unwrap_err()
            .side,
        Side::Consumer
    );
}

#[test]
fn a_structurally_equal_consumer_checks_once() {
    let methods = json!({ "rows": method(json!({ "kind": "string" }), json!({ "kind": "int" })) });
    let binding = ContractBinding::new(compile(methods.clone()), Some(compile(methods)));
    assert!(!binding.checks_both());
    assert_eq!(
        binding
            .validate_argument("rows", &json!(1))
            .unwrap_err()
            .side,
        Side::Provider
    );
}

#[test]
fn compiles_from_serialized_text() {
    let schema = ContractSchema::parse(
        r#"{ "b": { "kind": "fn", "argument": { "kind": "void" }, "result": { "kind": "json" } },
             "a": { "kind": "fn", "argument": { "kind": "null" }, "result": { "kind": "void" } } }"#,
    )
    .unwrap();
    assert_eq!(schema.methods().collect::<Vec<_>>(), ["a", "b"]);
    assert!(ContractSchema::parse("{").is_err());
}

#[test]
fn malformed_descriptors_name_where_they_fail() {
    let string = json!({ "kind": "string" });
    let cases = [
        (
            json!([]),
            "invalid contract descriptor: methods must be an object",
        ),
        (
            json!({}),
            "invalid contract descriptor: a contract needs at least one method",
        ),
        (
            json!({ "m": { "kind": "string" } }),
            "invalid contract descriptor at m: expected kind \"fn\", found \"string\"",
        ),
        (
            json!({ "m": { "kind": "fn", "argument": string } }),
            "invalid contract descriptor at m: missing key \"result\"",
        ),
        (
            json!({ "m": method(json!({ "kind": "strin" }), string.clone()) }),
            "invalid contract descriptor at m.argument: unknown schema kind \"strin\"",
        ),
        (
            json!({ "m": method(json!({ "kind": "array", "items": { "kind": "void" } }), string.clone()) }),
            "invalid contract descriptor at m.argument.items: void is only a method argument or result",
        ),
        (
            json!({ "m": method(json!({ "kind": "optional", "schema": string }), string.clone()) }),
            "invalid contract descriptor at m.argument: optional is only an object field",
        ),
        (
            json!({ "m": method(json!({ "kind": "string", "extra": 1 }), string.clone()) }),
            "invalid contract descriptor at m.argument: unexpected key \"extra\"",
        ),
        (
            json!({ "m": method(json!({ "kind": "literal", "value": [] }), string.clone()) }),
            "invalid contract descriptor at m.argument: a literal must be a string, number, boolean, or null",
        ),
        (
            json!({ "m": method(json!({ "kind": "enum", "values": ["a", "a"] }), string.clone()) }),
            "invalid contract descriptor at m.argument: duplicate enum value \"a\"",
        ),
        (
            json!({ "m": method(json!({ "kind": "enum", "values": [] }), string.clone()) }),
            "invalid contract descriptor at m.argument: an enum needs at least one value",
        ),
        (
            json!({ "m": method(json!({ "kind": "union", "options": [] }), string.clone()) }),
            "invalid contract descriptor at m.argument: a union needs at least one option",
        ),
        (
            json!({ "m": method(
                json!({ "kind": "tuple", "items": [string, { "kind": "fn" }] }),
                string.clone(),
            ) }),
            "invalid contract descriptor at m.argument.items.1: fn is only a contract method",
        ),
        (
            json!({ "m": method(
                json!({ "kind": "object", "fields": { "a": { "kind": "optional", "schema": { "kind": "void" } } } }),
                string.clone(),
            ) }),
            "invalid contract descriptor at m.argument.fields.a.schema: void is only a method argument or result",
        ),
        (
            json!({ "m": method(json!({ "kind": "void", "extra": 1 }), string) }),
            "invalid contract descriptor at m.argument: unexpected key \"extra\"",
        ),
    ];
    for (methods, expected) in cases {
        assert_eq!(descriptor_error(methods), expected);
    }
}
