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
pub const KW_LET: &str = "let";

/// `(let! NAME VALUE)` — variable binding wrapped in a fresh ref.
pub const KW_LET_REF: &str = "let!";

/// `(fn NAME PARAMS BODY)` — function definition.
pub const KW_FN: &str = "fn";

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

/// `(try BODY (catch VAR HANDLER...) [(finally CLEANUP...)])` — evaluate
/// BODY, catching any value raised by `(raise ...)`. `catch`/`finally` are
/// recognized only positionally inside `try`, so they are not reserved.
pub const KW_TRY: &str = "try";

/// `(exception NAME)` — bind NAME to an exception constructor that builds a
/// tagged cons `('NAME arg...)`. A special form (not a macro) because it
/// introduces a binding in the caller's env.
pub const KW_EXCEPTION: &str = "exception";

/// `(open PATH [PREFIX])` — load a module and merge all its bindings into the
/// caller; with an optional `PREFIX` ident, merged names become `PREFIX.NAME`.
pub const KW_OPEN: &str = "open";

/// `(load PATH)` — load a module and return its bindings as a map keyed by ident.
pub const KW_LOAD: &str = "load";

/// `(load-quoted PATH)` — read a file and return its top-level forms as a list of data.
pub const KW_LOAD_QUOTED: &str = "load-quoted";

/// Separator joining a module `PREFIX` to a binding name in `(open PATH PREFIX)`.
pub const MODULE_PREFIX_SEP: char = '.';

/// `(doc ARG+)` — documentation slot for binding forms. Not a standalone
/// special form; meaningful only in the optional doc position of `let`,
/// `let!`, `fn`, `defmacro`.
pub const KW_DOC: &str = "doc";
