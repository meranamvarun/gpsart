use gpsart_domain::templates::{
    Catalogue, Category, TemplateError, embedded_catalogue, load_template, render_svg,
};

fn valid_template() -> serde_json::Value {
    serde_json::from_str(include_str!("../templates/heart.v1.json")).unwrap()
}

#[test]
fn rejects_invalid_template_geometry_and_rules() {
    let mut cases = Vec::new();

    let mut empty = valid_template();
    empty["strokes"][0]["points"] = serde_json::json!([]);
    cases.push((empty, TemplateError::EmptyStroke { index: 0 }));

    let mut invalid_join =
        serde_json::from_str::<serde_json::Value>(include_str!("../templates/hi.v1.json")).unwrap();
    invalid_join["transitions"][0]["to_stroke"] = serde_json::json!(3);
    cases.push((invalid_join, TemplateError::InvalidTransition { index: 0 }));

    let mut disconnected_join =
        serde_json::from_str::<serde_json::Value>(include_str!("../templates/hi.v1.json")).unwrap();
    disconnected_join["transitions"][0]["points"][0] = serde_json::json!({"x": 0.13, "y": 0.15});
    cases.push((
        disconnected_join,
        TemplateError::InvalidTransition { index: 0 },
    ));

    let mut orientation = valid_template();
    orientation["orientation"]["rule"] = serde_json::json!("sideways_magic");
    assert!(matches!(
        load_template(&orientation.to_string()),
        Err(TemplateError::Malformed(_))
    ));

    let mut too_complex = valid_template();
    too_complex["complexity"]["max_points"] = serde_json::json!(1);
    cases.push((too_complex, TemplateError::ComplexityExceeded));

    for (value, expected) in cases {
        assert_eq!(load_template(&value.to_string()).unwrap_err(), expected);
    }

    let non_finite = valid_template().to_string().replacen("0.5", "1e999", 1);
    assert!(load_template(&non_finite).is_err());
}

#[test]
fn text_is_north_up_ordered_and_never_mirrored() {
    let mut text =
        serde_json::from_str::<serde_json::Value>(include_str!("../templates/hi.v1.json")).unwrap();
    for (field, value) in [
        ("mirror", serde_json::json!(true)),
        ("stroke_reordering", serde_json::json!(true)),
    ] {
        text["orientation"][field] = value;
        assert_eq!(
            load_template(&text.to_string()).unwrap_err(),
            TemplateError::InvalidTextOrientation
        );
        text = serde_json::from_str(include_str!("../templates/hi.v1.json")).unwrap();
    }
    text["orientation"]["max_degrees"] = serde_json::json!(15.01);
    assert_eq!(
        load_template(&text.to_string()).unwrap_err(),
        TemplateError::InvalidTextOrientation
    );
}

#[test]
fn versions_are_monotonic_and_immutable_by_hash() {
    let heart = include_str!("../templates/heart.v1.json");
    assert!(matches!(
        Catalogue::load(&[heart, heart]),
        Err(TemplateError::DuplicateVersion { .. })
    ));

    let mut v3 = valid_template();
    v3["version"] = serde_json::json!(3);
    assert!(matches!(
        Catalogue::load(&[&v3.to_string(), heart]),
        Err(TemplateError::NonMonotonicVersion { .. })
    ));
}

#[test]
fn complete_traversal_keeps_every_connector_and_retrace() {
    let catalogue = embedded_catalogue().unwrap();
    let hi = catalogue.get("HI", 1).unwrap();
    let traversal = hi.complete_traversal();
    assert_eq!(
        traversal.len(),
        hi.definition.strokes.len() + hi.definition.transitions.len()
    );
    assert!(
        traversal
            .iter()
            .any(|part| part.classification() == "connector")
    );
    assert!(
        traversal
            .iter()
            .any(|part| part.classification() == "retrace")
    );
}

#[test]
fn published_catalogue_filters_drafts_and_category() {
    let mut draft = valid_template();
    draft["version"] = serde_json::json!(2);
    draft["status"] = serde_json::json!("draft");
    let draft_source = draft.to_string();
    let catalogue = Catalogue::load(&[
        include_str!("../templates/heart.v1.json"),
        &draft_source,
        include_str!("../templates/hi.v1.json"),
    ])
    .unwrap();
    assert_eq!(
        catalogue
            .published(None)
            .iter()
            .map(|t| t.definition.id.as_str())
            .collect::<Vec<_>>(),
        ["heart", "HI"]
    );
    assert_eq!(catalogue.published(Some(Category::Text)).len(), 1);
    assert!(
        catalogue
            .published(None)
            .iter()
            .all(|item| item.definition.status.as_str() == "published")
    );
}

#[test]
fn published_hashes_and_svg_renders_are_golden() {
    let catalogue = embedded_catalogue().unwrap();
    for template in catalogue.published(None) {
        let stem = format!(
            "{}.v{}",
            template.definition.id.to_ascii_lowercase(),
            template.definition.version
        );
        let expected_hash = match stem.as_str() {
            "heart.v1" => include_str!("fixtures/heart.v1.sha256").trim(),
            "hi.v1" => include_str!("fixtures/hi.v1.sha256").trim(),
            _ => panic!("published template lacks a reviewed golden fixture: {stem}"),
        };
        let expected_svg = match stem.as_str() {
            "heart.v1" => include_str!("fixtures/heart.v1.svg"),
            "hi.v1" => include_str!("fixtures/hi.v1.svg"),
            _ => unreachable!(),
        };
        assert_eq!(template.definition_hash, expected_hash);
        assert_eq!(
            render_svg(&template.definition),
            expected_svg.replace("\r\n", "\n")
        );
    }
}
