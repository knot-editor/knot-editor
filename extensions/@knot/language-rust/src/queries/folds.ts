// Nodes whose lines fold. A region ends before a closing bracket's line.
export const rust = String.raw`
[
  (block)
  (declaration_list)
  (field_declaration_list)
  (ordered_field_declaration_list)
  (enum_variant_list)
  (field_initializer_list)
  (match_block)
  (use_list)
  (arguments)
  (parameters)
  (array_expression)
  (token_tree)
  (block_comment)
] @fold
`;
