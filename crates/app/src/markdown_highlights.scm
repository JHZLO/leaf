; Markdown's block highlights: gpui-kit's query (from nvim-treesitter) with what it left plain.
; Tables, task boxes and quote markers are marked like list markers, and link definitions use the
; link names the theme styles (gpui-kit names them text.uri and text.reference, which it does not).
(atx_heading (inline) @title)
(setext_heading (paragraph) @title)

[
  (atx_h1_marker)
  (atx_h2_marker)
  (atx_h3_marker)
  (atx_h4_marker)
  (atx_h5_marker)
  (atx_h6_marker)
  (setext_h1_underline)
  (setext_h2_underline)
] @punctuation.special

[
  (link_title)
  (indented_code_block)
  (fenced_code_block)
] @text.literal

[
  (fenced_code_block_delimiter)
] @punctuation.delimiter

(code_fence_content) @none

[
  (link_destination)
] @link_uri

[
  (link_label)
] @link_text

[
  (list_marker_plus)
  (list_marker_minus)
  (list_marker_star)
  (list_marker_dot)
  (list_marker_parenthesis)
  (thematic_break)
  (task_list_marker_checked)
  (task_list_marker_unchecked)
  (block_quote_marker)
  (pipe_table_delimiter_row)
  "|"
] @punctuation.list_marker

(pipe_table_header (pipe_table_cell) @emphasis.strong)

[
  (block_continuation)
] @punctuation.special

[
  (backslash_escape)
] @string.escape
