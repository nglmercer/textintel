#!/usr/bin/env python3
"""Bounded offline decision search. Select on validation, then freeze and test.

Uses only Python's standard library and the two compiled Rust binaries.
Original datasets/models are never overwritten. Outputs are under target/.
"""
import argparse
import hashlib
import itertools
import json
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
    temporary.replace(path)


def read_jsonl(path):
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def write_jsonl(path, rows):
    path.write_text("".join(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n" for row in rows))


def example_identity(example):
    return (example["state"].casefold(), json.dumps(example["question"], sort_keys=True))


def augment(rows):
    result, seen = [], set()
    for example in rows:
        state = example["state"]
        for index, text in enumerate([state, state.lower(), state.upper(), state.translate(str.maketrans("aeiosAEIOS", "4310543105"))]):
            identity = (text, json.dumps(example["question"], sort_keys=True))
            if identity in seen:
                continue
            seen.add(identity)
            result.append(dict(example, state=text, id=f'{example.get("id", "train")}-view{index}'))
    return result


def matching_rows(split):
    source = json.loads((ROOT / "data/evaluation" / f"{split}.json").read_text())
    return [
        {
            "id": f'matching-{case["id"]}', "state": case["a"],
            "question": {"type": "choice", "instructions": "Does MATCH express the same meaning as the message?",
                         "criteria": {"MATCH": case["b"], "NONE_OF_THE_ABOVE": "No candidate expresses the same meaning as the message."}},
            "gold": "MATCH" if case.get("labels", {}).get("similar", False) else "NONE_OF_THE_ABOVE",
            "task": "pair-matching",
        } for case in source["cases"]
    ]


def prepare_dataset(task, output):
    directory = output / task / "dataset"
    directory.mkdir(parents=True, exist_ok=True)
    if task == "routing":
        train = read_jsonl(ROOT / "data/decision/train.jsonl")
        valid = read_jsonl(ROOT / "data/decision/validation.jsonl")
        version = "support-routing-seed-0.1.0"
    else:
        train, valid = matching_rows("train"), matching_rows("validation")
        version = "pair-matching-from-evaluation-0.9.0"
    overlap = {example_identity(row) for row in train} & {example_identity(row) for row in valid}
    if overlap:
        raise ValueError(f"{task}: {len(overlap)} train/validation examples overlap")
    write_json(directory / "dataset.json", {"version": version})
    write_jsonl(directory / "train.jsonl", train)
    write_jsonl(directory / "validation.jsonl", valid)
    # The evaluator loads all splits. Keep test empty throughout selection.
    (directory / "test.jsonl").write_text("")
    write_jsonl(directory / "augmented.jsonl", augment(train))
    return directory, len(train), len(valid)


def commands(config, dataset, model, trainer, evaluator, cache):
    train = [str(trainer), "--embeddings", config["encoder"], "--train", str(dataset / config["training"]),
             "--valid", str(dataset / "validation.jsonl"), "--out", str(model),
             "--hidden", str(config["hidden"]), "--lr", str(config["lr"]), "--seed", str(config["seed"]),
             "--epochs", "100", "--patience", "8", "--batch", "32", "--cache", str(cache), "--jobs", "2", "--no-rebus"]
    if config.get("approach") == "prototype":
        train += ["--approach", "prototype", "--prototype-blend", str(config["blend"])]
    if not config["fusion"]:
        train.append("--no-fusion")
    evaluate = [str(evaluator), "eval-decision", str(dataset), "--split", "validation", "--provider", config.get("approach", "interaction"),
                "--head", str(model), "--embeddings", config["encoder"], "--no-rebus", "--json"]
    return train, evaluate


def execute(command, stdout, remaining):
    with stdout.open("w") as stream:
        subprocess.run(command, cwd=ROOT, stdout=stream, stderr=subprocess.PIPE, check=True,
                       timeout=max(0.1, remaining))


def ranking(trial):
    metrics = trial["validation"]
    return (-metrics["macro_f1"], metrics["nll"], trial["parameters"], json.dumps(trial["config"], sort_keys=True))


def search(task, args, deadline):
    dataset, train_count, valid_count = prepare_dataset(task, args.output)
    task_dir = args.output / task
    baseline_file = task_dir / "baseline-validation.json"
    execute([str(args.evaluator), "eval-decision", str(dataset), "--split", "validation", "--json", "--no-rebus"],
            baseline_file, min(180, deadline - time.monotonic()))
    baseline = json.loads(baseline_file.read_text())
    encoders = ["hash:64", "wordhash:64", "hash:128", "wordhash:128"]
    if task == "routing" and args.approach == "prototype":
        configs = [dict(encoder=e, hidden=0, lr=0, fusion=False, training=t, seed=0, approach="prototype", blend=b)
                   for e, t, b in itertools.product(["hash:128", "hash:256", "hash:512", "wordhash:128", "wordhash:256", "wordhash:512"],
                                                   ["train.jsonl", "augmented.jsonl"], [0.0, 0.25, 0.5, 0.75, 1.0])]
    elif task == "routing":
        configs = [dict(encoder=e, hidden=h, lr=lr, fusion=f, training=t, seed=s)
                   for e, h, lr, f, t, s in itertools.product(encoders, [8, 16], [0.003, 0.01],
                                                           [False, True], ["train.jsonl", "augmented.jsonl"], [7, 19])]
    else:
        configs = [dict(encoder=e, hidden=h, lr=lr, fusion=f, training="train.jsonl", seed=7)
                   for e, h, lr, f in itertools.product(encoders, [8, 16], [0.003, 0.01, 0.03], [False, True])]
    trials = []
    for config in configs:
        if deadline - time.monotonic() < 20:
            break
        identity = hashlib.sha256(json.dumps(config, sort_keys=True).encode()).hexdigest()[:16]
        directory = task_dir / "trials" / identity
        directory.mkdir(parents=True, exist_ok=True)
        model = directory / "head.json"
        train_command, valid_command = commands(config, dataset, model, args.trainer, args.evaluator, task_dir / "cache")
        started = time.monotonic()
        try:
            execute(train_command, directory / "training.log", min(180, deadline - time.monotonic()))
            execute(valid_command, directory / "validation.json", min(180, deadline - time.monotonic()))
            artifact = json.loads(model.read_text())
            report = json.loads((directory / "validation.json").read_text())
            if report["count"] != valid_count or report["skipped"]:
                raise ValueError("validation did not execute every selected example")
            trial = {"config": config, "validation": report, "model": str(model),
                     "seconds": time.monotonic() - started,
                     "parameters": (sum(len(vector) for vector in artifact["prototypes"].values()) if config.get("approach") == "prototype"
                                    else len(artifact["w1"]) + len(artifact["b1"]) + len(artifact["w2"]) + 1)}
            trials.append(trial)
            print(f'{task} trial={len(trials)} {config} validation_f1={report["macro_f1"]:.4f} nll={report["nll"]:.4f}', flush=True)
        except (subprocess.SubprocessError, ValueError) as error:
            write_json(directory / "failure.json", {"error": str(error), "config": config})
            print(f"{task}: trial failed: {error}", flush=True)
        write_json(task_dir / "search.json", {"task": task, "train_count": train_count, "validation_count": valid_count,
                                               "selection": "highest validation macro-F1, then NLL, then parameter count, then deterministic configuration order",
                                               "baseline_validation": baseline, "trials": trials})
    if not trials:
        raise RuntimeError(f"{task}: no successful trials within the time budget")
    winner = min(trials, key=ranking)
    # Persist selection BEFORE touching held-out inputs. Never try a runner-up
    # because its test score looks better; test cannot select configuration.
    write_json(task_dir / "selection.json", winner)
    if task == "routing":
        test = read_jsonl(ROOT / "data/decision/test.jsonl")
    else:
        test = matching_rows("test")
    train = read_jsonl(dataset / "train.jsonl")
    valid = read_jsonl(dataset / "validation.jsonl")
    overlap = ({example_identity(row) for row in train} | {example_identity(row) for row in valid}) & {example_identity(row) for row in test}
    if overlap:
        raise ValueError(f"{task}: {len(overlap)} held-out examples overlap fitting data")
    write_jsonl(dataset / "test.jsonl", test)
    _, command = commands(winner["config"], dataset, Path(winner["model"]), args.trainer, args.evaluator, task_dir / "cache")
    command[command.index("--split") + 1] = "test"
    # Reserve bounded time for reporting after the search budget expires.
    execute(command, task_dir / "selected-test.json", 180)
    execute([str(args.evaluator), "eval-decision", str(dataset), "--split", "test", "--json", "--no-rebus"],
            task_dir / "baseline-test.json", 180)
    selected_test = json.loads((task_dir / "selected-test.json").read_text())
    baseline_test = json.loads((task_dir / "baseline-test.json").read_text())
    if selected_test["count"] != len(test) or selected_test["skipped"]:
        raise ValueError("held-out evaluation did not execute every selected example")
    result = {"task": task, "trials": len(trials), "selection": winner,
              "baseline_test": baseline_test, "selected_test": selected_test,
              "test_selected_configuration": False}
    write_json(task_dir / "result.json", result)
    print(f'{task}: frozen held-out accuracy {baseline_test["accuracy"]:.4f} -> {selected_test["accuracy"]:.4f}', flush=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seconds", type=float, default=1800, help="Total search budget; final test reporting reserves up to 6 additional minutes per task.")
    parser.add_argument("--approach", choices=["interaction", "prototype"], default="interaction")
    parser.add_argument("--task", choices=["routing", "matching", "both"], default="both")
    parser.add_argument("--output", type=Path, default=ROOT / "target/decision-search")
    parser.add_argument("--trainer", type=Path, default=ROOT / "target/release/textintel-train-decision")
    parser.add_argument("--evaluator", type=Path, default=ROOT / "target/release/textintel")
    args = parser.parse_args()
    if args.seconds <= 0:
        parser.error("--seconds must be positive")
    if args.approach == "prototype" and args.task != "routing":
        parser.error("prototype search is restricted to --task routing (fixed criteria)")
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    deadline = started + args.seconds
    tasks = ["routing", "matching"] if args.task == "both" else [args.task]
    results = []
    for index, task in enumerate(tasks):
        # Routing is small; reserve most of a combined budget for pair matching.
        task_deadline = min(deadline, time.monotonic() + args.seconds * 0.25) if index == 0 and len(tasks) == 2 else deadline
        results.append(search(task, args, task_deadline))
    write_json(args.output / "results.json", {"elapsed_seconds": time.monotonic() - started, "tasks": results})


if __name__ == "__main__":
    main()
