//! Out-of-distribution handling: energy scores, entropy gaps, and the
//! explicit `NONE_OF_THE_ABOVE` criterion convention.

use std::collections::BTreeMap;
use std::sync::Arc;

use textintel::decision::{
    DecisionQuestion, DecisionRequest, NONE_OF_THE_ABOVE, SimilarityDecisionProvider, energy_score,
    entropy,
};
use textintel::{ProfileSimilarityScorer, TextIntelligence};

#[test]
fn none_of_the_above_is_a_first_class_criterion() {
    assert_eq!(NONE_OF_THE_ABOVE, "NONE_OF_THE_ABOVE");
    let engine = TextIntelligence::default().with_decision_provider(
        SimilarityDecisionProvider::new(Arc::new(ProfileSimilarityScorer::default())),
    );
    let request = DecisionRequest::new(
        "I need help resetting my password",
        DecisionQuestion::Choice {
            instructions: "Which team?".to_string(),
            criteria: BTreeMap::from([
                ("billing".to_string(), "Payments and refunds.".to_string()),
                (
                    NONE_OF_THE_ABOVE.to_string(),
                    "None of the other options applies.".to_string(),
                ),
            ]),
        },
    );
    let response = engine.decide(&request).expect("decide");
    response.validate_against(&request).expect("valid answer");
    // The plumbing carries the explicit unknown option end to end.
    let probabilities = response.answer.probabilities_in_order();
    assert_eq!(probabilities.len(), 2);
}

#[test]
fn flat_distributions_look_more_ood_than_peaked_ones() {
    let flat_entropy = entropy(&[0.5, 0.5]).expect("entropy");
    let peaked_entropy = entropy(&[0.99, 0.01]).expect("entropy");
    assert!(flat_entropy > peaked_entropy);

    let flat_energy = energy_score(&[0.0, 0.0], 1.0).expect("energy");
    let peaked_energy = energy_score(&[4.0, 0.0], 1.0).expect("energy");
    assert!(
        flat_energy > peaked_energy,
        "flat logits read more OOD ({flat_energy} vs {peaked_energy})"
    );
}

#[test]
fn unrelated_inputs_score_flatter_than_topical_ones() {
    // Behavioral anchor (not a guarantee): against billing/technical
    // references, an on-topic billing message should concentrate mass
    // while an off-topic message spreads it.
    let engine = TextIntelligence::default().with_decision_provider(
        SimilarityDecisionProvider::new(Arc::new(ProfileSimilarityScorer::default())),
    );
    let question = || DecisionQuestion::Choice {
        instructions: "Which team?".to_string(),
        criteria: BTreeMap::from([
            (
                "billing".to_string(),
                "Payments, invoices, refunds and charges.".to_string(),
            ),
            (
                "technical".to_string(),
                "Errors, crashes, bugs and login failures.".to_string(),
            ),
        ]),
    };
    let topical = engine
        .decide(&DecisionRequest::new(
            "refund my duplicate invoice payment for the subscription charge",
            question(),
        ))
        .expect("decide");
    let unrelated = engine
        .decide(&DecisionRequest::new(
            "giraffes eat acacia leaves at sunset",
            question(),
        ))
        .expect("decide");
    let topical_entropy = entropy(&topical.answer.probabilities_in_order()).expect("entropy");
    let unrelated_entropy = entropy(&unrelated.answer.probabilities_in_order()).expect("entropy");
    assert!(
        unrelated_entropy >= topical_entropy,
        "unrelated ({unrelated_entropy:.3}) should spread mass vs topical ({topical_entropy:.3})"
    );
}
