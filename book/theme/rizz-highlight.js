// rizz syntax highlighting for mdbook.
//
// mdbook bundles highlight.js, which ships no Lisp grammar, so rizz code fenced
// as ```clojure renders uncoloured by default. This script teaches the bundled
// engine a rizz grammar (registered under "clojure") and re-highlights the
// affected blocks. The token classification mirrors the tree-sitter grammar at
// tree-sitter-rizz/queries/highlights.scm.
//
// It runs after mdbook's own highlight pass (both scripts execute at end of
// <body>); the bundled highlight.js (v10) leaves unknown-language blocks as
// plain text, so re-running highlightBlock now that "clojure" is registered
// colours them. Everything is wrapped in try/catch so any failure degrades to
// the previous plain-text rendering rather than breaking the page.
(function () {
  "use strict";
  if (typeof hljs === "undefined") return;

  function rizzGrammar(hljs) {
    // A rizz identifier: does not start with a reader-macro prefix and contains
    // no whitespace or delimiters (lexer terminators: whitespace ( ) [ ] { } ;).
    var SYMBOL_RE = "[a-zA-Z_+\\-*/<>=!?.%&|^~$][^\\s()\\[\\]{};\"'`,@]*";

    // Special forms (head-position keywords in the spec).
    var SPECIAL = "let let! set! deref ref fn if do quote quasi unquote " +
      "unquote-splice eval defmacro open doc show load load-quoted " +
      "try catch finally exception raise";
    // Prelude control-flow / combinator macros — a second colour.
    var CONTROL = "cond match unless for loop while else and or";

    var KEYWORDS = { $pattern: SYMBOL_RE, keyword: SPECIAL, "built_in": CONTROL };

    var NUMBER = { className: "number", begin: "-?\\d+(\\.\\d*)?", relevance: 0 };
    var STRING = {
      className: "string", begin: '"', end: '"',
      contains: [{ className: "subst", begin: "\\\\[\\\\\"nrt]" }], relevance: 0
    };
    var COMMENT = hljs.COMMENT(";;", "$");
    var READER = { className: "meta", begin: "(,@|['`,])" };
    // The first symbol after '(' is a call head; KEYWORDS upgrades the known
    // special forms / control macros to their own colours.
    var HEAD = {
      className: "title",
      begin: "(?<=\\()\\s*" + SYMBOL_RE,
      keywords: KEYWORDS, relevance: 0
    };

    return {
      name: "rizz",
      aliases: ["rizz"],
      keywords: KEYWORDS,
      contains: [COMMENT, STRING, NUMBER, READER, HEAD]
    };
  }

  try {
    hljs.registerLanguage("clojure", rizzGrammar);
    var blocks = document.querySelectorAll("code.language-clojure");
    for (var i = 0; i < blocks.length; i++) {
      var el = blocks[i];
      if (el.dataset) delete el.dataset.highlighted; // be friendly to any v11
      hljs.highlightBlock(el);
    }
  } catch (e) {
    if (window.console) console.warn("rizz highlighting failed:", e);
  }
})();
