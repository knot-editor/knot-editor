// Nodes whose lines fold. A region ends before a closing bracket's line.
export const ecma = String.raw`
[
  (statement_block)
  (class_body)
  (switch_body)
  (object)
  (array)
  (arguments)
  (formal_parameters)
  (named_imports)
  (export_clause)
  (template_string)
  (comment)
] @fold
`;

export const typescript = String.raw`
[
  (interface_body)
  (enum_body)
  (object_type)
] @fold
`;

export const jsx = String.raw`
[
  (jsx_element)
  (jsx_self_closing_element)
] @fold
`;
