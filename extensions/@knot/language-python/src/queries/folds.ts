// Nodes whose lines fold. An indented block folds from the line that opens
// it, so a compound statement's header stays visible.
export const python = String.raw`
[
  (block)
  (list)
  (dictionary)
  (set)
  (tuple)
  (argument_list)
  (parameters)
  (string)
] @fold
`;
