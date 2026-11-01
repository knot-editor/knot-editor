// Adapted from nvim-treesitter (Apache-2.0); see extensions/@knot/LICENSE-APACHE.
export const highlights = String.raw`
; Types
; Javascript
; Variables
;-----------
(identifier) @variable

; Properties
;-----------
(property_identifier) @variable.member

(shorthand_property_identifier) @variable.member

(private_property_identifier) @variable.member

(object_pattern
  (shorthand_property_identifier_pattern) @variable)

(object_pattern
  (object_assignment_pattern
    (shorthand_property_identifier_pattern) @variable))

; Special identifiers
;--------------------
((identifier) @type
  (#match? @type "^[A-Z]"))

((identifier) @constant
  (#match? @constant "^_*[A-Z][A-Z0-9_]*$"))

((shorthand_property_identifier) @constant
  (#match? @constant "^_*[A-Z][A-Z0-9_]*$"))

((identifier) @variable.builtin
  (#any-of? @variable.builtin "arguments" "module" "console" "window" "document"))

((identifier) @type.builtin
  (#any-of? @type.builtin
    "Object" "Function" "Boolean" "Symbol" "Number" "Math" "Date" "String" "RegExp" "Map" "Set"
    "WeakMap" "WeakSet" "Promise" "Array" "Int8Array" "Uint8Array" "Uint8ClampedArray" "Int16Array"
    "Uint16Array" "Int32Array" "Uint32Array" "Float32Array" "Float64Array" "ArrayBuffer" "DataView"
    "Error" "EvalError" "InternalError" "RangeError" "ReferenceError" "SyntaxError" "TypeError"
    "URIError"))

(statement_identifier) @label

; Function and method definitions
;--------------------------------
(function_expression
  name: (identifier) @function)

(function_declaration
  name: (identifier) @function)

(generator_function
  name: (identifier) @function)

(generator_function_declaration
  name: (identifier) @function)

(method_definition
  name: [
    (property_identifier)
    (private_property_identifier)
  ] @function.method)

(method_definition
  name: (property_identifier) @constructor
  (#eq? @constructor "constructor"))

(pair
  key: (property_identifier) @function.method
  value: (function_expression))

(pair
  key: (property_identifier) @function.method
  value: (arrow_function))

(assignment_expression
  left: (member_expression
    property: (property_identifier) @function.method)
  right: (arrow_function))

(assignment_expression
  left: (member_expression
    property: (property_identifier) @function.method)
  right: (function_expression))

(variable_declarator
  name: (identifier) @function
  value: (arrow_function))

(variable_declarator
  name: (identifier) @function
  value: (function_expression))

(assignment_expression
  left: (identifier) @function
  right: (arrow_function))

(assignment_expression
  left: (identifier) @function
  right: (function_expression))

; Function and method calls
;--------------------------
(call_expression
  function: (identifier) @function.call)

(call_expression
  function: (member_expression
    property: [
      (property_identifier)
      (private_property_identifier)
    ] @function.method.call))

(call_expression
  function: (await_expression
    (identifier) @function.call))

(call_expression
  function: (await_expression
    (member_expression
      property: [
        (property_identifier)
        (private_property_identifier)
      ] @function.method.call)))

; Builtins
;---------
((identifier) @module.builtin
  (#eq? @module.builtin "Intl"))

((identifier) @function.builtin
  (#any-of? @function.builtin
    "eval" "isFinite" "isNaN" "parseFloat" "parseInt" "decodeURI" "decodeURIComponent" "encodeURI"
    "encodeURIComponent" "require"))

; Constructor
;------------
(new_expression
  constructor: (identifier) @constructor)

; Decorators
;----------
(decorator
  "@" @attribute
  (identifier) @attribute)

(decorator
  "@" @attribute
  (call_expression
    (identifier) @attribute))

(decorator
  "@" @attribute
  (member_expression
    (property_identifier) @attribute))

(decorator
  "@" @attribute
  (call_expression
    (member_expression
      (property_identifier) @attribute)))

; Literals
;---------
[
  (this)
  (super)
] @variable.builtin

((identifier) @variable.builtin
  (#eq? @variable.builtin "self"))

[
  (true)
  (false)
] @boolean

[
  (null)
  (undefined)
] @constant.builtin

[
  (comment)
  (html_comment)
] @comment

((comment) @comment.documentation
  (#match? @comment.documentation "^/[*][*][^*].*[*]/$"))

(hash_bang_line) @keyword.directive

((string_fragment) @keyword.directive
  (#eq? @keyword.directive "use strict"))

(string) @string

(template_string) @string

(escape_sequence) @string.escape

(regex_pattern) @string.regexp

(regex_flags) @character.special

(regex
  "/" @punctuation.bracket) ; Regex delimiters

(number) @number

((identifier) @number
  (#any-of? @number "NaN" "Infinity"))

; Punctuation
;------------
[
  ";"
  "."
  ","
  ":"
] @punctuation.delimiter

[
  "--"
  "-"
  "-="
  "&&"
  "+"
  "++"
  "+="
  "&="
  "/="
  "**="
  "<<="
  "<"
  "<="
  "<<"
  "="
  "=="
  "==="
  "!="
  "!=="
  "=>"
  ">"
  ">="
  ">>"
  "||"
  "%"
  "%="
  "*"
  "**"
  ">>>"
  "&"
  "|"
  "^"
  "??"
  "*="
  ">>="
  ">>>="
  "^="
  "|="
  "&&="
  "||="
  "??="
  "..."
] @operator

(binary_expression
  "/" @operator)

(ternary_expression
  [
    "?"
    ":"
  ] @keyword.conditional.ternary)

(unary_expression
  [
    "!"
    "~"
    "-"
    "+"
  ] @operator)

(unary_expression
  [
    "delete"
    "void"
  ] @keyword.operator)

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

(template_substitution
  [
    "${"$"}{"
    "}"
  ] @punctuation.special)

; Imports
;----------
(namespace_import
  "*" @character.special
  (identifier) @module)

(namespace_export
  "*" @character.special
  (identifier) @module)

(export_statement
  "*" @character.special)

; Keywords
;----------
[
  "if"
  "else"
  "switch"
  "case"
] @keyword.conditional

[
  "import"
  "from"
  "as"
  "export"
] @keyword.import

[
  "for"
  "of"
  "do"
  "while"
  "continue"
] @keyword.repeat

[
  "break"
  "const"
  "debugger"
  "extends"
  "get"
  "let"
  "set"
  "static"
  "target"
  "var"
  "with"
] @keyword

"class" @keyword.type

[
  "async"
  "await"
] @keyword.coroutine

[
  "return"
  "yield"
] @keyword.return

"function" @keyword.function

[
  "new"
  "delete"
  "in"
  "instanceof"
  "typeof"
] @keyword.operator

[
  "throw"
  "try"
  "catch"
  "finally"
] @keyword.exception

(export_statement
  "default" @keyword)

(switch_default
  "default" @keyword.conditional)
`;

// Parse errors are left to Knot's unclosed-bracket rule rather than
// auto-indented, so a block typed before its closer still indents.
export const indents = String.raw`
[
  (arguments)
  (array)
  (binary_expression)
  (class_body)
  (export_clause)
  (formal_parameters)
  (named_imports)
  (object)
  (object_pattern)
  (parenthesized_expression)
  (return_statement)
  (statement_block)
  (switch_case)
  (switch_default)
  (switch_statement)
  (template_substitution)
  (ternary_expression)
] @indent.begin

(arguments
  (call_expression) @indent.begin)

(binary_expression
  (call_expression) @indent.begin)

(expression_statement
  (call_expression) @indent.begin)

(arrow_function
  body: (_) @_body
  (#not-kind-eq? @_body "statement_block")) @indent.begin

(assignment_expression
  right: (_) @_right
  (#not-kind-eq? @_right "arrow_function")) @indent.begin

(variable_declarator
  value: (_) @_value
  (#not-kind-eq? @_value "arrow_function" "call_expression")) @indent.begin

(arguments
  ")" @indent.end)

(object
  "}" @indent.end)

(statement_block
  "}" @indent.end)

[
  (arguments
    (object))
  ")"
  "}"
  "]"
] @indent.branch

(statement_block
  "{" @indent.branch)

((parenthesized_expression
  "("
  (_)
  ")" @indent.end) @_outer
  (#not-has-parent? @_outer if_statement))

[
  "}"
  "]"
] @indent.end

(template_string) @indent.ignore

(comment) @indent.auto

(if_statement
  consequence: (_) @indent.dedent
  (#not-kind-eq? @indent.dedent statement_block)) @indent.begin
`;
