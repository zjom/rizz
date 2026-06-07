//! Language-wide string constants.
//!
//! - `FILE_EXTENSION` — the suffix `(open ...)` appends to a path with no
//!   extension.
//! - `KW_*` — identifiers that the evaluator treats as special forms when
//!   they appear in **head position** of a list. They are reserved purely
//!   lexically: shadowing them with a `let` binds a value, but a call
//!   `(let ...)` still dispatches as the special form (the runtime checks
//!   the head string before doing env lookup).

/// File extension appended to extensionless paths passed to `(open ...)`.
pub const FILE_EXTENSION: &str = "rz";

/// `(let NAME VALUE)` — variable binding. See [`crate::runtime::eval`].
pub const KW_DEFVAR: &str = "let";
/// `(let! NAME VALUE)` — variable binding wrapped in a fresh ref.
pub const KW_DEFVAR_REF: &str = "let!";
/// `(fn NAME PARAMS BODY)` — function definition.
pub const KW_DEFUN: &str = "fn";
/// `(defmacro NAME PARAMS BODY)` — user-defined macro.
pub const KW_DEFMACRO: &str = "defmacro";
/// `(quote X)` — return `X` unevaluated. Also: `'X`.
pub const KW_QUOTE: &str = "quote";
/// `(quasi X)` — quasiquote: literal except for `unquote` subforms. Also: `` `X ``.
pub const KW_QUASIQUOTE: &str = "quasi";
/// `(unquote X)` — within `quasi`, evaluate `X`. Also: `,X`.
pub const KW_UNQUOTE: &str = "unquote";
/// `(unquote-splice X)` — within `quasi`, splice the result of `X`. Also: `,@X`.
pub const KW_UNQUOTE_SPLICE: &str = "unquote-splice";
/// `(if COND THEN ELSE)` — conditional.
pub const KW_IF: &str = "if";
/// `(do FORM*)` — pure sequencing.
pub const KW_DO: &str = "do";
/// `(eval FORM)` — evaluate a runtime-built form.
pub const KW_EVAL: &str = "eval";
/// `(open PATH)` — load a module and merge its bindings.
pub const KW_OPEN: &str = "open";
/// `(doc ARG+)` — documentation slot for binding forms. Not a standalone
/// special form; meaningful only in the optional doc position of `let`,
/// `let!`, `fn`, `defmacro`.
pub const KW_DOC: &str = "doc";
