; Identifiers

(identifier) @variable

((identifier) @type
  (#match? @type "^[A-Z]"))

((identifier) @constant
  (#match? @constant "^[A-Z][A-Z0-9_]+$"))

((identifier) @type.builtin
  (#any-of? @type.builtin
    "type" "void" "error" "bool" "int" "float" "str" "byte" "symbol" "any" "dyn"
    "i8" "i16" "i32" "i64" "r8" "r16" "r32" "r64" "f32" "f64"))

((identifier) @variable.builtin
  (#eq? @variable.builtin "self"))

(at_identifier) @function.builtin

(template_parameter) @type

(discard) @variable.builtin

; Declarations

(function_definition
  name: (_) @function)

(function_definition
  parent: (_) @type)

(function_definition
  receiver: (receiver) @type)

(parameter
  name: (identifier) @variable.parameter)

(infer_lambda
  (parameter) @variable.parameter)

(capture
  (identifier) @variable.parameter)

(type_declaration
  name: (_) @type.definition)

(type_alias
  name: (_) @type.definition)

(field_declaration
  name: (_) @property)

(case_declaration
  name: (_) @constant)

(const_declaration
  name: (_) @constant)

(global_declaration
  name: (_) @variable)

(variable_declaration
  name: (identifier) @variable)

(use_statement
  name: (module_path) @module)

(attribute
  name: (identifier) @attribute)

(attribute_list
  ["#[" "]"] @attribute)

(named_argument
  name: (identifier) @variable.parameter)

(pair
  key: (identifier) @property)

; Expressions

(call_expression
  function: (identifier) @function.call)

(call_expression
  function: (member_expression
    member: (identifier) @function.method.call))

(member_expression
  member: (identifier) @property)

(unwrap_choice
  case: (_) @constant)

(symbol_literal) @constant

(error_literal) @constant

(special_string
  prefix: (identifier) @string.special)

; Literals

(number) @number

[
  (true)
  (false)
] @boolean

[
  (none)
  (undef)
] @constant.builtin

(string) @string

(raw_string) @string

(escape_sequence) @string.escape

(interpolation
  ["%{" "}"] @punctuation.special)

(comment) @comment

(shebang) @keyword.directive

; Keywords

[
  "fn"
  "fnsym"
] @keyword.function

(generator_fn) @keyword.function

[
  "type"
  "struct"
  "cstruct"
  "cunion"
  "enum"
  "trait"
] @keyword.type

[
  "var"
  "const"
  "global"
  "with"
  "scope"
  "sink"
  "move"
  "begin"
] @keyword

"use" @keyword.import

[
  "return"
  "yield"
] @keyword.return

[
  "fiber"
  "spawn"
  (suspend_statement)
] @keyword.coroutine

(break_statement) @keyword.repeat

(continue_statement) @keyword.repeat

(pass_statement) @keyword

[
  "if"
  "else"
  "#else"
  "switch"
  "case"
] @keyword.conditional

[
  "for"
  "while"
] @keyword.repeat

[
  "try"
  "?else"
  "!else"
] @keyword.exception

[
  "and"
  "or"
  "not"
  "as"
] @keyword.operator

(visibility) @keyword.modifier

(comptime_statement
  "#" @keyword.directive)

(comptime_expression
  ["#{" "}"] @keyword.directive)

; Operators

[
  "+" "-" "*" "/" "%" "**"
  "==" "!=" "<" "<=" ">" ">="
  "<<" ">>" "&&" "||" "~" "!" "^" "&" "?"
  "=" ":=" "+=" "-=" "*=" "/=" "<<=" ">>=" "||=" "&&=" "~="
  ".." "..=" "..>" "..>="
  ".?" ".!" ".*"
  "->" "=>" "::"
] @operator

; Punctuation

[
  "("
  ")"
  "["
  "]"
  "[]"
  "{"
  "}"
] @punctuation.bracket

(capture
  "|" @punctuation.bracket)

(infer_lambda
  "|" @punctuation.bracket)

[
  ","
  "."
  ":"
] @punctuation.delimiter
