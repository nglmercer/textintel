//! CLI surface declarations: one [`CliSpec`] per binary. Help text,
//! usage lines, and parser tables all derive from these constants —
//! handlers read values by id and never repeat flag spellings.

use super::spec::{ArgKind, ArgSpec, CliSpec, CommandSpec, PositionalSpec};

const fn flag(id: &'static str, long: &'static str, help: &'static str) -> ArgSpec {
    ArgSpec {
        id,
        long,
        kind: ArgKind::Flag,
        help,
    }
}

const fn value(
    id: &'static str,
    long: &'static str,
    metavar: &'static str,
    help: &'static str,
) -> ArgSpec {
    ArgSpec {
        id,
        long,
        kind: ArgKind::Value { metavar },
        help,
    }
}

const fn positional(
    id: &'static str,
    metavar: &'static str,
    required: bool,
    rest: bool,
    help: &'static str,
) -> PositionalSpec {
    PositionalSpec {
        id,
        metavar,
        required,
        rest,
        help,
    }
}

// ---------------------------------------------------------------------------
// textintel
// ---------------------------------------------------------------------------

const TEXTINTEL_ARGS: &[ArgSpec] = &[
    value(
        "locale",
        "locale",
        "CODE",
        "Explicit speech language (e.g. es, en, zh).",
    ),
    value(
        "date-order",
        "date-order",
        "ORDER",
        "Numeric date order: dmy|mdy|reject (default reject).",
    ),
    value(
        "decimal-separator",
        "decimal-separator",
        "CHAR",
        "Explicit decimal separator: dot|comma|arabic; unset preserves ambiguity.",
    ),
    flag("json", "json", "Emit machine-readable JSON output."),
    flag(
        "production",
        "production",
        "Use the local production preset (never downloads or networks).",
    ),
    value(
        "resource-root",
        "resource-root",
        "DIR",
        "Load resource packs from DIR instead of the embedded ones.",
    ),
    value(
        "language",
        "language",
        "CODE",
        "Preferred language hint (BCP-47).",
    ),
    value(
        "languages",
        "languages",
        "A,B",
        "Preferred language hints, comma-separated.",
    ),
    value("threshold", "threshold", "0..1", "Decision threshold."),
    value(
        "mode",
        "mode",
        "MODE",
        "Duplicate mode: combined|near_exact|lexical|semantic|phonetic|decoded|visual.",
    ),
    value(
        "split",
        "split",
        "SPLIT",
        "Split to score: train|validation|test.",
    ),
    value(
        "profile",
        "profile",
        "NAME",
        "Similarity profile: general|duplicate|spam|obfuscation|rebus|search.",
    ),
    value(
        "gates",
        "gates",
        "FILE",
        "Quality-gates file (exit 1 on failure).",
    ),
    value(
        "spam-corpus",
        "spam-corpus",
        "FILE",
        "Held-out spam corpus.",
    ),
    flag("no-ranking", "no-ranking", "Skip the retrieval probe."),
];

const TEXTINTEL_COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "parse-speech",
        aliases: &[],
        summary: "Parse lossless typed spans for speech.",
        args: &["locale", "date-order", "decimal-separator"],
        required_args: &["locale"],
        positionals: &[positional(
            "text",
            "TEXT",
            true,
            false,
            "Original text to parse.",
        )],
    },
    CommandSpec {
        name: "normalize-speech",
        aliases: &[],
        summary: "Produce explicit spoken readings and unresolved-span diagnostics.",
        args: &["locale", "date-order", "decimal-separator"],
        required_args: &["locale"],
        positionals: &[positional(
            "text",
            "TEXT",
            true,
            false,
            "Original text for speech.",
        )],
    },
    CommandSpec {
        name: "eval-speech",
        aliases: &[],
        summary: "Measure exact speech spans and annotated spoken-form accuracy.",
        args: &[],
        required_args: &[],
        positionals: &[positional(
            "dataset",
            "CORPUS.JSON",
            false,
            false,
            "Speech corpus (default data/speech/evaluation.json).",
        )],
    },
    CommandSpec {
        name: "analyze",
        aliases: &[],
        summary: "Analyze a message across every channel.",
        args: &["resource-root", "language"],
        required_args: &[],
        positionals: &[positional(
            "text",
            "TEXT",
            true,
            false,
            "Message to analyze.",
        )],
    },
    CommandSpec {
        name: "explain",
        aliases: &[],
        summary: "Analyze a message and explain rebus decodings.",
        args: &["resource-root", "language"],
        required_args: &[],
        positionals: &[positional(
            "text",
            "TEXT",
            true,
            false,
            "Message to explain.",
        )],
    },
    CommandSpec {
        name: "decode",
        aliases: &[],
        summary: "Decode rebus/leet/symbol readings of a message.",
        args: &["resource-root", "languages"],
        required_args: &[],
        positionals: &[positional(
            "text",
            "TEXT",
            true,
            false,
            "Message to decode.",
        )],
    },
    CommandSpec {
        name: "compare",
        aliases: &[],
        summary: "Score the similarity of two messages.",
        args: &["resource-root", "language"],
        required_args: &[],
        positionals: &[
            positional("a", "MESSAGE-A", true, false, "First message."),
            positional("b", "MESSAGE-B", true, false, "Second message."),
        ],
    },
    CommandSpec {
        name: "duplicate",
        aliases: &[],
        summary: "Judge whether two messages are duplicates.",
        args: &["resource-root", "threshold", "mode"],
        required_args: &[],
        positionals: &[
            positional("a", "MESSAGE-A", true, false, "First message."),
            positional("b", "MESSAGE-B", true, false, "Second message."),
        ],
    },
    CommandSpec {
        name: "spam",
        aliases: &[],
        summary: "Score spam/abuse probability of a message.",
        args: &["resource-root"],
        required_args: &[],
        positionals: &[positional("text", "TEXT", true, false, "Message to score.")],
    },
    CommandSpec {
        name: "batch",
        aliases: &[],
        summary: "Analyze one message per line of a JSONL file.",
        args: &["resource-root"],
        required_args: &[],
        positionals: &[positional(
            "input",
            "INPUT.JSONL",
            true,
            false,
            "Input file, one message per line.",
        )],
    },
    CommandSpec {
        name: "resources",
        aliases: &[],
        summary: "Inspect resource packs (or validate one file).",
        args: &[],
        required_args: &[],
        positionals: &[
            positional(
                "root",
                "ROOT",
                false,
                false,
                "Resource root (default: resources), or 'validate'.",
            ),
            positional(
                "path",
                "PATH",
                false,
                false,
                "File to validate when ROOT is 'validate'.",
            ),
        ],
    },
    CommandSpec {
        name: "diagnostics",
        aliases: &[],
        summary: "Report providers, caches, and storage capabilities.",
        args: &["resource-root"],
        required_args: &[],
        positionals: &[],
    },
    CommandSpec {
        name: "provider-info",
        aliases: &[],
        summary: "Alias for the diagnostics report.",
        args: &["resource-root"],
        required_args: &[],
        positionals: &[],
    },
    CommandSpec {
        name: "schema-version",
        aliases: &[],
        summary: "Print API and fingerprint schema versions.",
        args: &[],
        required_args: &[],
        positionals: &[],
    },
    CommandSpec {
        name: "eval",
        aliases: &["evaluate"],
        summary: "Score an evaluation dataset and enforce quality gates.",
        args: &["split", "profile", "gates", "spam-corpus", "no-ranking"],
        required_args: &[],
        positionals: &[positional(
            "dataset",
            "DATASET",
            false,
            false,
            "Dataset dir or file (default: data/evaluation).",
        )],
    },
    CommandSpec {
        name: "index",
        aliases: &[],
        summary: "Index one document into a JSON store.",
        args: &["resource-root", "language"],
        required_args: &[],
        positionals: &[
            positional("store", "STORE.JSON", true, false, "Store file."),
            positional("id", "ID", true, false, "Document id."),
            positional("text", "TEXT", true, true, "Document text (rest of line)."),
        ],
    },
    CommandSpec {
        name: "search",
        aliases: &[],
        summary: "Search a JSON store for similar texts.",
        args: &["resource-root"],
        required_args: &[],
        positionals: &[
            positional("store", "STORE.JSON", true, false, "Store file."),
            positional("text", "TEXT", true, false, "Query text."),
            positional("limit", "LIMIT", true, false, "Maximum results."),
        ],
    },
];

const TEXTINTEL_SPEC: &CliSpec = &CliSpec {
    name: "textintel",
    about: "Local-first multilingual text intelligence.",
    args: TEXTINTEL_ARGS,
    global_args: &["json", "production"],
    commands: TEXTINTEL_COMMANDS,
    single_positionals: &[],
    single_required_args: &[],
    footer: Some(
        "JSON output versions are reported by schema-version; consult the changelog for API changes.",
    ),
};

/// Declarative surface of the `textintel` binary.
pub fn textintel_spec() -> &'static CliSpec {
    TEXTINTEL_SPEC
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn library_cli_spec_validates() {
        textintel_spec().validate().unwrap();
    }
    #[test]
    fn cli_contains_only_library_commands() {
        let names: Vec<_> = textintel_spec().commands.iter().map(|c| c.name).collect();
        assert_eq!(
            names,
            vec![
                "parse-speech",
                "normalize-speech",
                "eval-speech",
                "analyze",
                "explain",
                "decode",
                "compare",
                "duplicate",
                "spam",
                "batch",
                "resources",
                "diagnostics",
                "provider-info",
                "schema-version",
                "eval",
                "index",
                "search"
            ]
        );
    }
}
