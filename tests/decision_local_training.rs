use std::collections::BTreeMap;
use textintel::decision::{
    DecisionProvider, DecisionQuestion, DecisionRequest, HeadTrainExample, HeadTrainer,
    InteractionArtifact, InteractionDecisionProvider, PrototypeArtifact, PrototypeDecisionProvider,
    init_head_xavier, interaction_features, open_decision_embeddings,
};
use textintel::{EngineConfig, TextIntelligence};

fn question() -> DecisionQuestion {
    DecisionQuestion::Choice {
        instructions: "Route the request".into(),
        criteria: BTreeMap::from([
            ("billing".into(), "payment refund invoice".into()),
            ("technical".into(), "crash error software".into()),
        ]),
    }
}

#[test]
fn indexed_adam_matches_cloned_batches_and_rejects_bad_indices() {
    let data: Vec<_> = (0..4)
        .map(|index| {
            HeadTrainExample::new(
                vec![
                    vec![index as f32 / 4.0, 0.25, -0.1, 0.5],
                    vec![0.5, -0.25, index as f32 / 8.0, -0.1],
                ],
                index % 2,
            )
            .unwrap()
        })
        .collect();
    let mut cloned = init_head_xavier(4, 3, 7).unwrap();
    let mut indexed = cloned.clone();
    let mut left = HeadTrainer::new(&cloned, 0.01, 3).unwrap();
    let mut right = HeadTrainer::new(&indexed, 0.01, 3).unwrap();
    for indices in [&[2, 0, 3][..], &[1, 2][..], &[3, 1, 0][..]] {
        let batch: Vec<_> = indices.iter().map(|index| data[*index].clone()).collect();
        assert_eq!(
            left.step(&mut cloned, &batch).unwrap(),
            right.step_indexed(&mut indexed, &data, indices).unwrap()
        );
        assert_eq!(cloned, indexed);
    }
    assert!(right.step_indexed(&mut indexed, &data, &[9]).is_err());
    assert_eq!(
        cloned, indexed,
        "invalid indices must not update parameters"
    );
    assert!(right.step_indexed(&mut indexed, &data, &[]).is_err());
}

#[test]
fn embedding_only_decisions_skip_analysis_but_keep_input_bounds() {
    let embeddings = open_decision_embeddings("wordhash:16").unwrap();
    let head = init_head_xavier(64, 4, 7).unwrap();
    let mut artifact = InteractionArtifact::from_head(&head, 16, 0, "unit").unwrap();
    artifact.embedding_model = embeddings.model_metadata();
    artifact.temperature = 0.75;
    let provider = InteractionDecisionProvider::new(embeddings, &artifact).unwrap();
    assert!(!provider.needs_fingerprints());
    let engine = TextIntelligence::default();
    let mut request = DecisionRequest::new("refund my payment", question());
    engine
        .prepare_decision_request_with_provider(&provider, &mut request)
        .unwrap();
    assert!(request.fingerprint.is_none());
    assert!(request.candidate_fingerprints.is_empty());
    provider
        .decide(&request)
        .unwrap()
        .validate_against(&request)
        .unwrap();
    let config = EngineConfig {
        max_input_length: 5,
        ..EngineConfig::default()
    };
    let bounded = TextIntelligence::new(config).with_decision_provider(provider);
    assert!(bounded.decide(&request).is_err());
    assert!(
        InteractionDecisionProvider::new(open_decision_embeddings("hash:16").unwrap(), &artifact)
            .is_err()
    );
    artifact.temperature = 0.0;
    assert!(artifact.validate().is_err());
}

#[test]
fn prototype_roundtrip_calibrates_and_enforces_its_task_and_backbone() {
    let embeddings = open_decision_embeddings("wordhash:64").unwrap();
    let criteria = match question() {
        DecisionQuestion::Choice { criteria, .. } => criteria,
        _ => unreachable!(),
    };
    let descriptions: Vec<_> = criteria.values().cloned().collect();
    let candidate_vectors = embeddings.embed(&descriptions).unwrap();
    let rows: Vec<_> = ["payment refund invoice", "crash error software"]
        .iter()
        .enumerate()
        .map(|(gold, state)| {
            let vector = embeddings.embed(&[state.to_string()]).unwrap();
            HeadTrainExample::new(
                candidate_vectors
                    .iter()
                    .map(|candidate| interaction_features(&vector[0], candidate, None).unwrap())
                    .collect(),
                gold,
            )
            .unwrap()
        })
        .collect();
    let artifact = PrototypeArtifact::fit(
        embeddings.model_metadata().unwrap(),
        criteria,
        &rows,
        &rows,
        0.5,
        "unit".into(),
    )
    .unwrap();
    let serialized = serde_json::to_string(&artifact).unwrap();
    let decoded: PrototypeArtifact = serde_json::from_str(&serialized).unwrap();
    let provider = PrototypeDecisionProvider::new(embeddings, decoded).unwrap();
    let engine = TextIntelligence::default().with_decision_provider(provider);
    for (state, expected) in [
        ("payment refund invoice", "billing"),
        ("crash error software", "technical"),
    ] {
        let request = DecisionRequest::new(state, question());
        let response = engine.decide(&request).unwrap();
        response.validate_against(&request).unwrap();
        assert_eq!(response.answer.predicted_label(), expected);
    }
    let mut wrong = question();
    if let DecisionQuestion::Choice { criteria, .. } = &mut wrong {
        criteria.insert("other".into(), "other category".into());
    }
    assert!(
        engine
            .decide(&DecisionRequest::new("payment", wrong))
            .is_err()
    );
    assert!(
        PrototypeDecisionProvider::new(open_decision_embeddings("hash:64").unwrap(), artifact)
            .is_err()
    );
    for spec in ["hash:0", "hash:99999999", "wordhash:0", "wordhash:invalid"] {
        assert!(open_decision_embeddings(spec).is_err());
    }
}

#[test]
fn fused_decisions_skip_candidate_analysis_with_identical_probabilities() {
    let embeddings = open_decision_embeddings("wordhash:16").unwrap();
    let fusion = textintel::decision::FUSION_FEATURES.len();
    let head = init_head_xavier(64 + fusion, 4, 7).unwrap();
    let artifact = InteractionArtifact::from_head(&head, 16, fusion, "unit").unwrap();
    let provider = InteractionDecisionProvider::new(embeddings, &artifact).unwrap();
    assert!(provider.needs_fingerprints());
    assert!(!provider.needs_candidate_fingerprints());
    let engine = TextIntelligence::default();
    let mut full = DecisionRequest::new("refund my payment", question());
    let mut lean = full.clone();
    engine.prepare_decision_request(&mut full).unwrap();
    engine
        .prepare_decision_request_with_provider(&provider, &mut lean)
        .unwrap();
    assert!(lean.fingerprint.is_some());
    assert!(lean.candidate_fingerprints.is_empty());
    assert_eq!(
        provider.decide(&full).unwrap(),
        provider.decide(&lean).unwrap()
    );
}
