// Adapted from nvim-treesitter (Apache-2.0); see extensions/@knot/LICENSE-APACHE.
// Raw strings are ignored rather than auto-indented, since their lines'
// indentation is content, and block comments move with their first line.
export const rust = String.raw`
[
  (mod_item)
  (struct_item)
  (enum_item)
  (impl_item)
  (struct_expression)
  (struct_pattern)
  (tuple_struct_pattern)
  (tuple_expression)
  (tuple_type)
  (tuple_pattern)
  (match_block)
  (call_expression)
  (assignment_expression)
  (arguments)
  (block)
  (where_clause)
  (use_list)
  (array_expression)
  (ordered_field_declaration_list)
  (field_declaration_list)
  (enum_variant_list)
  (parameters)
  (token_tree)
  (token_repetition)
  (macro_definition)
] @indent.begin

(trait_item
  body: (_) @indent.begin)

(string_literal
  (escape_sequence)) @indent.begin

; Typing "(" inside a macro definition breaks the tree entirely, turning
; macro_definition into an error. Offset this by adding back one indent for
; the start of the macro rules.
(ERROR
  .
  "macro_rules!"
  [
    "("
    "{"
    "["
  ] @indent.begin
  (#set! indent.immediate)
  (#set! indent.start_at_same_line))

(macro_definition
  [
    ")"
    "}"
    "]"
  ] @indent.end)

(block
  "}" @indent.end)

(enum_item
  body: (enum_variant_list
    "}" @indent.end))

(impl_item
  body: (declaration_list
    "}" @indent.end))

(match_expression
  body: (match_block
    "}" @indent.end))

(mod_item
  body: (declaration_list
    "}" @indent.end))

(struct_item
  body: (field_declaration_list
    "}" @indent.end))

(struct_expression
  body: (field_initializer_list
    "}" @indent.end))

(struct_pattern
  "}" @indent.end)

(tuple_struct_pattern
  ")" @indent.end)

(tuple_type
  ")" @indent.end)

(tuple_pattern
  ")" @indent.end)

(trait_item
  body: (declaration_list
    "}" @indent.end))

(impl_item
  (where_clause) @indent.dedent)

[
  "where"
  ")"
  "]"
  "}"
] @indent.branch

(impl_item
  (declaration_list) @indent.branch)

[
  (line_comment)
  (string_literal)
  (raw_string_literal)
] @indent.ignore

(block_comment) @indent.auto
`;
