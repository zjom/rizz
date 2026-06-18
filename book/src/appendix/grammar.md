# Grammar

An informal summary of rizz's surface syntax. For the full prose, see
[Syntax](../language/syntax.md) and *SPEC.md* §2.

## Tokens

```text
whitespace  =  ' ' | '\t' | '\r' | '\n'           ; separates tokens
comment     =  ';;' .* (newline | EOF)            ; a lone ';' is an error

int         =  '-'? digit+                        ; 64-bit signed
float       =  '-'? digit+ '.' digit*             ; 64-bit IEEE-754; '1.' = 1.0
string      =  '"' (char | escape)* '"'           ; UTF-8
escape      =  '\\' | '\"' | '\n' | '\r' | '\t'   ; any other \x is an error
ident       =  run of bytes until whitespace or one of ( ) [ ] { } ;
```

A leading `-` immediately followed by a digit starts a number; otherwise `-`
begins an identifier. Identifiers may contain `+ - * / < > = ? ! .` and more, so
`str->int`, `empty?`, `set!`, and `foo.bar` are each a single identifier.

## Forms

```text
form     =  atom | list | array | map | quoted

atom     =  int | float | string | ident | '(' ')'   ; () is nil / Unit

list     =  '(' form* ')'                 ; a call or special form when evaluated
         |  '(' form+ '.' form ')'        ; dotted / improper list

array    =  '[' form* ']'                 ; whitespace-separated elements

map      =  '{' entry* '}'                ; entry = form ':' form
                                          ; entries whitespace-separated

quoted   =  "'"  form                     ; (quote form)
         |  '`'  form                     ; (quasi form)
         |  ','  form                     ; (unquote form)
         |  ',@' form                     ; (unquote-splice form)
```

## Notes

- The empty list `()` parses to **nil** (Unit), which is also the empty list and
  a falsy value.
- A standalone `.` (surrounded by whitespace) inside a list introduces a
  [dotted list](../language/syntax.md); exactly one form may follow it. The dot
  rule never interferes with floats (`1.5`) or dotted identifiers (`foo.bar`).
- Arrays use `[]`, maps use `{}` with `:` separating key from value. There are no
  commas between elements — whitespace separates them.
- The reader-macro prefixes `'`, `` ` ``, `,`, `,@` are pure syntax sugar that
  expand to the corresponding `quote` / `quasi` / `unquote` / `unquote-splice`
  forms.
- Parsing happens entirely before evaluation; any syntax error means no part of
  the program runs, and the error points at the offending line and column.

---

*See also:* [Syntax](../language/syntax.md) ·
[Reserved Identifiers](reserved.md) ·
[Standard Library Reference](stdlib-reference.md)
