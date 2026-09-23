//! Decision types: serde shapes, candidate labels, predictions, and the
//! selective-classification rule.

use std::collections::BTreeMap;

use textintel::decision::{
    Decision, DecisionAnswer, DecisionModelInfo, DecisionQuestion, DecisionRequest,
    DecisionResponse, selective_decision,
};

fn choice_question() -> DecisionQuestion {
    DecisionQuestion::Choice {
        instructions: "Which team should process this message?".to_string(),
        criteria: BTreeMap::from([
            (
                "billing".to_string(),
                "Payments, invoices and refunds".to_string(),
            ),
            (
                "technical".to_string(),
                "Product or technical problems".to_string(),
            ),
            (
                "sales".to_string(),
                "Purchasing or pricing questions".to_string(),
            ),
        ]),
    }
}

#[test]
fn question_json_matches_public_shapes() {
    let choice = serde_json::to_value(choice_question()).expect("serialize");
    assert_eq!(choice["type"], "choice");
    assert_eq!(
        choice["criteria"]["billing"],
        "Payments, invoices and refunds"
    );

    let binary = DecisionQuestion::Binary {
        statement: "This message is spam.".to_string(),
    };
    let json = serde_json::to_value(&binary).expect("serialize");
    assert_eq!(json["type"], "binary");
    let parsed: DecisionQuestion = serde_json::from_value(json).expect("deserialize");
    assert_eq!(parsed, binary);

    let score = DecisionQuestion::Score {
        instructions: "Rate severity.".to_string(),
        levels: vec!["none".to_string(), "low".to_string(), "high".to_string()],
    };
    let json = serde_json::to_value(&score).expect("serialize");
    assert_eq!(json["type"], "score");
    let parsed: DecisionQuestion = serde_json::from_value(json).expect("deserialize");
    assert_eq!(parsed, score);
}

#[test]
fn candidate_labels_follow_scoring_order() {
    assert_eq!(
        choice_question().candidate_labels(),
        vec!["billing", "sales", "technical"]
    );
    assert_eq!(
        DecisionQuestion::Binary {
            statement: "s".to_string()
        }
        .candidate_labels(),
        vec!["false", "true"]
    );
    assert_eq!(
        DecisionQuestion::Score {
            instructions: "s".to_string(),
            levels: vec!["a".to_string(), "b".to_string(), "c".to_string()],
        }
        .candidate_labels(),
        vec!["0", "1", "2"]
    );
}

#[test]
fn predicted_labels_match_winners() {
    let choice = DecisionAnswer::Choice {
        choice: "billing".to_string(),
        confidence: 0.91,
        probabilities: BTreeMap::from([
            ("billing".to_string(), 0.91),
            ("technical".to_string(), 0.06),
            ("sales".to_string(), 0.03),
        ]),
    };
    assert_eq!(choice.predicted_label(), "billing");
    assert_eq!(choice.confidence(), 0.91);

    let binary = DecisionAnswer::Binary {
        probability_true: 0.94,
        probability_false: 0.06,
        confidence: 0.94,
    };
    assert_eq!(binary.predicted_label(), "true");

    let score = DecisionAnswer::Score {
        expected_score: 0.2,
        confidence: 0.8,
        probabilities: vec![0.8, 0.2, 0.0],
    };
    assert_eq!(score.predicted_label(), "0");
}

#[test]
fn answer_validation_catches_mismatches() {
    let question = choice_question();
    // Unknown winner.
    let unknown = DecisionAnswer::Choice {
        choice: "nope".to_string(),
        confidence: 0.9,
        probabilities: BTreeMap::from([
            ("billing".to_string(), 0.9),
            ("technical".to_string(), 0.05),
            ("sales".to_string(), 0.05),
        ]),
    };
    assert!(unknown.validate_against(&question).is_err());
    // Probabilities that do not sum to 1.
    let bad_sum = DecisionAnswer::Choice {
        choice: "billing".to_string(),
        confidence: 0.5,
        probabilities: BTreeMap::from([
            ("billing".to_string(), 0.5),
            ("technical".to_string(), 0.1),
            ("sales".to_string(), 0.1),
        ]),
    };
    assert!(bad_sum.validate_against(&question).is_err());
    // Winner that is not the max.
    let wrong_winner = DecisionAnswer::Choice {
        choice: "sales".to_string(),
        confidence: 0.8,
        probabilities: BTreeMap::from([
            ("billing".to_string(), 0.8),
            ("technical".to_string(), 0.1),
            ("sales".to_string(), 0.1),
        ]),
    };
    assert!(wrong_winner.validate_against(&question).is_err());
    // Wrong answer type for the question.
    let binary = DecisionAnswer::Binary {
        probability_true: 0.5,
        probability_false: 0.5,
        confidence: 0.5,
    };
    assert!(binary.validate_against(&question).is_err());
    // NaN confidence.
    let nan = DecisionAnswer::Choice {
        choice: "billing".to_string(),
        confidence: f64::NAN,
        probabilities: BTreeMap::from([
            ("billing".to_string(), 0.8),
            ("technical".to_string(), 0.1),
            ("sales".to_string(), 0.1),
        ]),
    };
    assert!(nan.validate_against(&question).is_err());
}

#[test]
fn score_answers_check_expected_value() {
    let question = DecisionQuestion::Score {
        instructions: "Rate severity.".to_string(),
        levels: vec!["none".to_string(), "low".to_string(), "high".to_string()],
    };
    let valid = DecisionAnswer::Score {
        expected_score: 0.5,
        confidence: 0.6,
        probabilities: vec![0.6, 0.3, 0.1],
    };
    assert!(valid.validate_against(&question).is_ok());
    let inconsistent = DecisionAnswer::Score {
        expected_score: 2.0,
        confidence: 0.6,
        probabilities: vec![0.6, 0.3, 0.1],
    };
    assert!(inconsistent.validate_against(&question).is_err());
    let wrong_len = DecisionAnswer::Score {
        expected_score: 0.0,
        confidence: 1.0,
        probabilities: vec![1.0, 0.0],
    };
    assert!(wrong_len.validate_against(&question).is_err());
}

#[test]
fn selective_decision_accepts_above_threshold() {
    assert_eq!(selective_decision(0.91, 0.9), Decision::Accept);
    assert_eq!(selective_decision(0.9, 0.9), Decision::Accept);
    assert_eq!(selective_decision(0.89, 0.9), Decision::Escalate);
    // Fail closed on non-finite inputs.
    assert_eq!(selective_decision(f64::NAN, 0.5), Decision::Escalate);
    assert_eq!(selective_decision(0.99, f64::NAN), Decision::Escalate);
    assert_eq!(Decision::default(), Decision::Escalate);
}

#[test]
fn response_and_model_info_roundtrip() {
    let response = DecisionResponse::new(
        DecisionAnswer::Binary {
            probability_true: 0.94,
            probability_false: 0.06,
            confidence: 0.94,
        },
        Decision::Accept,
        "spam_decision_adapter",
    )
    .with_task("abuse")
    .with_model_revision("spam-v2");
    let json = serde_json::to_string(&response).expect("serialize");
    let parsed: DecisionResponse = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed, response);

    let info = DecisionModelInfo::new("similarity_decision_adapter", "specialist_adapter")
        .with_model("profile:general_similarity", None)
        .with_questions(["choice"]);
    let json = serde_json::to_string(&info).expect("serialize");
    let parsed: DecisionModelInfo = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed, info);
    assert!(parsed.local);
}

#[test]
fn question_validation_enforces_bounds() {
    assert!(choice_question().validate().is_ok());
    let too_many: BTreeMap<String, String> = (0..=textintel::MAX_DECISION_CANDIDATES)
        .map(|index| (format!("id{index}"), "description".to_string()))
        .collect();
    let oversized = DecisionQuestion::Choice {
        instructions: "Pick.".to_string(),
        criteria: too_many,
    };
    assert!(oversized.validate().is_err());
    let request = DecisionRequest::new("hello", choice_question());
    assert!(request.validate().is_ok());
}
