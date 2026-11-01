// Adapted from nvim-treesitter (Apache-2.0); see extensions/@knot/LICENSE-APACHE.
export const highlights = String.raw`
(bare_key) @property

[
  (string)
  (quoted_key)
] @string

(boolean) @boolean

(comment) @comment

(escape_sequence) @string.escape

(integer) @number

(float) @number.float

[
  (local_date)
  (local_date_time)
  (local_time)
  (offset_date_time)
] @string.special

"=" @operator

[
  "."
  ","
] @punctuation.delimiter

[
  "["
  "]"
  "[["
  "]]"
  "{"
  "}"
] @punctuation.bracket
`;

// Only closing brackets branch, so a nested array opening on its own line
// keeps its level.
export const indents = String.raw`
[
  (array)
  (inline_table)
] @indent.begin

[
  "]"
  "}"
] @indent.branch
`;
