// Nodes whose lines fold: each section from its heading, code fences, and
// list items.
export const markdown = String.raw`
[
  (section)
  (fenced_code_block)
  (list_item)
] @fold
`;
