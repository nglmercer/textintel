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
        "model-path",
        "model-path",
        "DIR",
        "Load trained artifacts (similarity/spam/reranker) from DIR.",
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
    value("scorer", "scorer", "FILE", "Trained similarity artifact."),
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
    value(
        "liquid-endpoint",
        "liquid-endpoint",
        "URL",
        "Local Liquid endpoint URL.",
    ),
    value("liquid-model", "liquid-model", "ID", "Liquid model id."),
    value(
        "max-tokens",
        "max-tokens",
        "N",
        "Maximum new tokens to generate.",
    ),
    value("temperature", "temperature", "F", "Sampling temperature."),
    value("system", "system", "TEXT", "System prompt for generation."),
    value(
        "provider",
        "provider",
        "NAME",
        "Decision provider: spam|similarity|interaction.",
    ),
    value(
        "task",
        "task",
        "NAME|PATH",
        "Decision task file or registry name.",
    ),
    value(
        "question",
        "question",
        "TEXT",
        "Override the task question text.",
    ),
    value("model-dir", "model-dir", "DIR", "Decision model directory."),
    value("head", "head", "FILE", "Interaction-head artifact."),
    value(
        "embeddings",
        "embeddings",
        "DIR",
        "Embedding backbone directory.",
    ),
    value(
        "jobs",
        "jobs",
        "N",
        "Eval worker threads (default 1; latencies under N>1 reflect contention).",
    ),
    flag(
        "no-rebus",
        "no-rebus",
        "Skip rebus decoding during analysis (fast decision serving over clean text).",
    ),
];

const TEXTINTEL_COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "analyze",
        aliases: &[],
        summary: "Analyze a message across every channel.",
        args: &["resource-root", "model-path", "language"],
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
        args: &["resource-root", "model-path", "language"],
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
        args: &["resource-root", "model-path", "languages"],
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
        args: &["resource-root", "model-path", "language"],
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
        args: &["resource-root", "model-path", "threshold", "mode"],
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
        args: &["resource-root", "model-path"],
        required_args: &[],
        positionals: &[positional("text", "TEXT", true, false, "Message to score.")],
    },
    CommandSpec {
        name: "batch",
        aliases: &[],
        summary: "Analyze one message per line of a JSONL file.",
        args: &["resource-root", "model-path"],
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
        summary: "Report providers, models, caches, and degraded capabilities.",
        args: &["resource-root", "model-path"],
        required_args: &[],
        positionals: &[],
    },
    CommandSpec {
        name: "provider-info",
        aliases: &[],
        summary: "Alias-style provider and model report (see diagnostics).",
        args: &["resource-root", "model-path"],
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
        args: &[
            "split",
            "profile",
            "scorer",
            "gates",
            "spam-corpus",
            "no-ranking",
        ],
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
        args: &["resource-root", "model-path", "language"],
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
        args: &["resource-root", "model-path"],
        required_args: &[],
        positionals: &[
            positional("store", "STORE.JSON", true, false, "Store file."),
            positional("text", "TEXT", true, false, "Query text."),
            positional("limit", "LIMIT", true, false, "Maximum results."),
        ],
    },
    CommandSpec {
        name: "generate",
        aliases: &[],
        summary: "Generate a completion with an explicit local model.",
        args: &[
            "resource-root",
            "model-path",
            "liquid-endpoint",
            "liquid-model",
            "max-tokens",
            "temperature",
            "system",
        ],
        required_args: &[],
        positionals: &[positional("prompt", "PROMPT", true, false, "Prompt text.")],
    },
    CommandSpec {
        name: "decide",
        aliases: &[],
        summary: "Answer a typed decision request file.",
        args: &[
            "resource-root",
            "model-path",
            "provider",
            "head",
            "embeddings",
            "threshold",
            "no-rebus",
        ],
        required_args: &[],
        positionals: &[positional(
            "request",
            "REQUEST.JSON",
            true,
            false,
            "DecisionRequest document.",
        )],
    },
    CommandSpec {
        name: "classify",
        aliases: &[],
        summary: "Classify text with a decision task file.",
        args: &[
            "resource-root",
            "model-path",
            "task",
            "question",
            "provider",
            "head",
            "embeddings",
            "no-rebus",
        ],
        required_args: &["task"],
        positionals: &[positional("text", "TEXT", true, false, "Text to classify.")],
    },
    CommandSpec {
        name: "decision-model-info",
        aliases: &[],
        summary: "Inspect a decision provider or model directory.",
        args: &["provider", "model-dir", "head", "embeddings"],
        required_args: &[],
        positionals: &[],
    },
    CommandSpec {
        name: "eval-decision",
        aliases: &[],
        summary: "Score a decision dataset and enforce decision gates.",
        args: &[
            "split",
            "provider",
            "gates",
            "head",
            "embeddings",
            "jobs",
            "no-rebus",
        ],
        required_args: &[],
        positionals: &[positional(
            "dataset",
            "DATASET",
            false,
            false,
            "Decision dataset dir or eval JSON (default: data/decision).",
        )],
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
        "JSON output contract: every --json payload follows API_VERSION (see schema-version); payloads evolve additively only — fields are added, never renamed or removed, within a major version.",
    ),
};

/// Declarative surface of the `textintel` binary.
pub fn textintel_spec() -> &'static CliSpec {
    TEXTINTEL_SPEC
}

// ---------------------------------------------------------------------------
// textintel-train
// ---------------------------------------------------------------------------

const TRAIN_ARGS: &[ArgSpec] = &[
    value("output", "output", "FILE", "Artifact to write (required)."),
    value("iterations", "iterations", "N", "Training iterations."),
    value("learning-rate", "learning-rate", "F", "Learning rate."),
    value("l2", "l2", "F", "L2 regularization strength."),
    value(
        "transformer-model",
        "transformer-model",
        "DIR",
        "Local transformer dir for featurization.",
    ),
    value(
        "count",
        "count",
        "N",
        "Synthetic spam messages to generate.",
    ),
    value("seed", "seed", "N", "Random seed."),
    value(
        "corpus",
        "corpus",
        "FILE",
        "Real spam corpus instead of synthetic data.",
    ),
    value("heldout", "heldout", "FILE", "Held-out spam eval corpus."),
];

const TRAIN_COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "similarity",
        aliases: &[],
        summary: "Train the logistic similarity scorer.",
        args: &[
            "output",
            "iterations",
            "learning-rate",
            "l2",
            "transformer-model",
        ],
        required_args: &["output"],
        positionals: &[positional(
            "dataset",
            "DATASET",
            false,
            false,
            "Evaluation dataset (default: data/evaluation).",
        )],
    },
    CommandSpec {
        name: "spam",
        aliases: &[],
        summary: "Train the logistic spam predictor.",
        args: &["output", "count", "seed", "corpus", "heldout"],
        required_args: &["output"],
        positionals: &[],
    },
];

const TRAIN_SPEC: &CliSpec = &CliSpec {
    name: "textintel-train",
    about: "Train interpretable logistic similarity and spam artifacts.",
    args: TRAIN_ARGS,
    global_args: &[],
    commands: TRAIN_COMMANDS,
    single_positionals: &[],
    single_required_args: &[],
    footer: None,
};

/// Declarative surface of the `textintel-train` binary.
pub fn train_spec() -> &'static CliSpec {
    TRAIN_SPEC
}

// ---------------------------------------------------------------------------
// textintel-train-decision
// ---------------------------------------------------------------------------

const TRAIN_DECISION_ARGS: &[ArgSpec] = &[
    value(
        "embeddings",
        "embeddings",
        "DIR",
        "Frozen embedding backbone directory (required).",
    ),
    value(
        "train",
        "train",
        "FILE[@N]",
        "Training JSONL, repeatable; @N upsamples (required).",
    ),
    value(
        "valid",
        "valid",
        "FILE",
        "Validation JSONL, repeatable (required).",
    ),
    value("out", "out", "FILE", "Head artifact to write (required)."),
    value("cache", "cache", "DIR", "Feature cache directory."),
    value("hidden", "hidden", "N", "Head hidden size (default: 128)."),
    value("lr", "lr", "F", "Adam learning rate (default: 0.001)."),
    value("batch", "batch", "N", "Mini-batch size (default: 32)."),
    value("epochs", "epochs", "N", "Maximum epochs (default: 20)."),
    value(
        "patience",
        "patience",
        "N",
        "Early-stop patience (default: 4).",
    ),
    value("seed", "seed", "N", "Deterministic seed (default: 7)."),
    value(
        "max-train",
        "max-train",
        "N",
        "Cap training examples (0 = all).",
    ),
    value(
        "jobs",
        "jobs",
        "N",
        "Extraction worker threads (default 1; features rejoin in order).",
    ),
    flag(
        "no-rebus",
        "no-rebus",
        "Skip rebus decoding during featurization (must match serving).",
    ),
];

const TRAIN_DECISION_SPEC: &CliSpec = &CliSpec {
    name: "textintel-train-decision",
    about: "Train a v2 interaction head over a frozen embedding backbone.",
    args: TRAIN_DECISION_ARGS,
    global_args: &[],
    commands: &[],
    single_positionals: &[],
    single_required_args: &["embeddings", "train", "valid", "out"],
    footer: None,
};

/// Declarative surface of the `textintel-train-decision` binary.
pub fn train_decision_spec() -> &'static CliSpec {
    TRAIN_DECISION_SPEC
}

// ---------------------------------------------------------------------------
// textintel-eval-llm
// ---------------------------------------------------------------------------

const EVAL_LLM_ARGS: &[ArgSpec] = &[
    value("eval", "eval", "FILE", "Shared eval JSON (required)."),
    value(
        "endpoint",
        "endpoint",
        "URL",
        "Chat endpoint base URL (required).",
    ),
    value("model", "model", "ID", "Model id to request (required)."),
    value("out", "out", "FILE", "Results JSON to write (required)."),
];

const EVAL_LLM_SPEC: &CliSpec = &CliSpec {
    name: "textintel-eval-llm",
    about: "Evaluate a chat endpoint on a shared choice-question eval set.",
    args: EVAL_LLM_ARGS,
    global_args: &[],
    commands: &[],
    single_positionals: &[],
    single_required_args: &["eval", "endpoint", "model", "out"],
    footer: None,
};

/// Declarative surface of the `textintel-eval-llm` binary.
pub fn eval_llm_spec() -> &'static CliSpec {
    EVAL_LLM_SPEC
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_specs_validate() {
        for spec in [
            textintel_spec(),
            train_spec(),
            train_decision_spec(),
            eval_llm_spec(),
        ] {
            spec.validate().expect("spec must validate");
        }
    }

    #[test]
    fn textintel_covers_every_shipped_command() {
        // Guards against handlers without spec entries (and vice versa):
        // keep this list in sync with the dispatch table in main.rs.
        let names: Vec<&str> = textintel_spec()
            .commands
            .iter()
            .map(|command| command.name)
            .collect();
        for expected in [
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
            "search",
            "generate",
            "decide",
            "classify",
            "decision-model-info",
            "eval-decision",
        ] {
            assert!(names.contains(&expected), "spec is missing {expected}");
        }
        assert_eq!(names.len(), 19, "dispatch table must match the spec");
    }
}
