use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use thiserror::Error;

const MAX_STROKES: usize = 32;
const MAX_POINTS: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Shape,
    Text,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Draft,
    Published,
}

impl Status {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    ForwardOnly,
    Either,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrientationRule {
    FreeRotation,
    NorthUpBounded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    Connector,
    Retrace,
}

impl TransitionKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Connector => "connector",
            Self::Retrace => "retrace",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stroke {
    pub id: String,
    pub direction: Direction,
    pub points: Vec<Point>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    pub from_stroke: usize,
    pub to_stroke: usize,
    pub classification: TransitionKind,
    pub points: Vec<Point>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Orientation {
    pub rule: OrientationRule,
    pub min_degrees: f64,
    pub max_degrees: f64,
    pub mirror: bool,
    pub stroke_reordering: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    pub min_scale: f64,
    pub max_scale: f64,
    pub min_aspect_ratio: f64,
    pub max_aspect_ratio: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Complexity {
    pub max_strokes: usize,
    pub max_points: usize,
    pub hint: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateDefinition {
    pub schema_version: u32,
    pub id: String,
    pub version: u32,
    pub status: Status,
    pub category: Category,
    pub name: String,
    pub strokes: Vec<Stroke>,
    pub transitions: Vec<Transition>,
    pub orientation: Orientation,
    pub bounds: Bounds,
    pub identifying_features: Vec<String>,
    pub complexity: Complexity,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VersionedTemplate {
    pub definition: TemplateDefinition,
    pub definition_hash: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TraversalPart<'a> {
    Stroke(&'a Stroke),
    Transition(&'a Transition),
}

impl TraversalPart<'_> {
    pub const fn classification(&self) -> &'static str {
        match self {
            Self::Stroke(_) => "intended_stroke",
            Self::Transition(value) => value.classification.as_str(),
        }
    }
}

impl VersionedTemplate {
    pub fn complete_traversal(&self) -> Vec<TraversalPart<'_>> {
        let mut parts =
            Vec::with_capacity(self.definition.strokes.len() + self.definition.transitions.len());
        for (index, stroke) in self.definition.strokes.iter().enumerate() {
            parts.push(TraversalPart::Stroke(stroke));
            if let Some(transition) = self.definition.transitions.get(index) {
                parts.push(TraversalPart::Transition(transition));
            }
        }
        parts
    }
}

#[derive(Clone, Debug)]
pub struct Catalogue {
    templates: Vec<VersionedTemplate>,
}

impl Catalogue {
    pub fn load(sources: &[&str]) -> Result<Self, TemplateError> {
        let mut templates = Vec::with_capacity(sources.len());
        let mut versions: BTreeMap<String, u32> = BTreeMap::new();
        for source in sources {
            let template = load_template(source)?;
            if let Some(previous) = versions.get(&template.definition.id) {
                if template.definition.version == *previous {
                    return Err(TemplateError::DuplicateVersion {
                        id: template.definition.id.clone(),
                        version: *previous,
                    });
                }
                if template.definition.version < *previous {
                    return Err(TemplateError::NonMonotonicVersion {
                        id: template.definition.id.clone(),
                    });
                }
            }
            versions.insert(template.definition.id.clone(), template.definition.version);
            templates.push(template);
        }
        Ok(Self { templates })
    }

    pub fn get(&self, id: &str, version: u32) -> Option<&VersionedTemplate> {
        self.templates
            .iter()
            .find(|item| item.definition.id == id && item.definition.version == version)
    }

    pub fn published(&self, category: Option<Category>) -> Vec<&VersionedTemplate> {
        self.templates
            .iter()
            .filter(|item| {
                item.definition.status == Status::Published
                    && category.is_none_or(|value| item.definition.category == value)
            })
            .collect()
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum TemplateError {
    #[error("template JSON is malformed: {0}")]
    Malformed(String),
    #[error("template identity and version must be non-empty and non-zero")]
    InvalidIdentity,
    #[error("stroke {index} is empty")]
    EmptyStroke { index: usize },
    #[error("coordinate at stroke {stroke}, point {point} is not finite or normalized")]
    NonFiniteCoordinate { stroke: usize, point: usize },
    #[error("transition {index} does not connect consecutive ordered strokes")]
    InvalidTransition { index: usize },
    #[error(
        "text orientation must remain ordered, unmirrored, north-up, and within plus or minus 15 degrees"
    )]
    InvalidTextOrientation,
    #[error("template scale or aspect bounds are invalid")]
    InvalidBounds,
    #[error("template exceeds its declared or platform complexity limit")]
    ComplexityExceeded,
    #[error("duplicate immutable version {id}@{version}")]
    DuplicateVersion { id: String, version: u32 },
    #[error("versions for {id} are not monotonically ordered")]
    NonMonotonicVersion { id: String },
}

pub fn load_template(source: &str) -> Result<VersionedTemplate, TemplateError> {
    let definition: TemplateDefinition = serde_json::from_str(source)
        .map_err(|error| TemplateError::Malformed(error.to_string()))?;
    validate(&definition)?;
    let canonical = serde_json::to_vec(&definition)
        .map_err(|error| TemplateError::Malformed(error.to_string()))?;
    let definition_hash = format!("{:x}", Sha256::digest(canonical));
    Ok(VersionedTemplate {
        definition,
        definition_hash,
    })
}

fn validate(definition: &TemplateDefinition) -> Result<(), TemplateError> {
    if definition.schema_version != 1
        || definition.id.trim().is_empty()
        || definition.name.trim().is_empty()
        || definition.version == 0
    {
        return Err(TemplateError::InvalidIdentity);
    }
    if definition.strokes.is_empty()
        || definition.strokes.len() > MAX_STROKES
        || definition.strokes.len() > definition.complexity.max_strokes
    {
        return Err(TemplateError::ComplexityExceeded);
    }
    let point_count = definition
        .strokes
        .iter()
        .map(|stroke| stroke.points.len())
        .sum::<usize>()
        + definition
            .transitions
            .iter()
            .map(|transition| transition.points.len())
            .sum::<usize>();
    if point_count > MAX_POINTS || point_count > definition.complexity.max_points {
        return Err(TemplateError::ComplexityExceeded);
    }
    for (stroke_index, stroke) in definition.strokes.iter().enumerate() {
        if stroke.points.is_empty() {
            return Err(TemplateError::EmptyStroke {
                index: stroke_index,
            });
        }
        validate_points(&stroke.points, stroke_index)?;
    }
    if definition.transitions.len() != definition.strokes.len().saturating_sub(1) {
        return Err(TemplateError::InvalidTransition {
            index: definition.transitions.len(),
        });
    }
    for (index, transition) in definition.transitions.iter().enumerate() {
        if transition.from_stroke != index
            || transition.to_stroke != index + 1
            || transition.points.is_empty()
        {
            return Err(TemplateError::InvalidTransition { index });
        }
        validate_points(&transition.points, transition.from_stroke)?;
        if transition.points.first() != definition.strokes[index].points.last()
            || transition.points.last() != definition.strokes[index + 1].points.first()
        {
            return Err(TemplateError::InvalidTransition { index });
        }
    }
    let orientation = &definition.orientation;
    if !orientation.min_degrees.is_finite()
        || !orientation.max_degrees.is_finite()
        || orientation.min_degrees > orientation.max_degrees
    {
        return Err(TemplateError::InvalidBounds);
    }
    if definition.category == Category::Text
        && (orientation.rule != OrientationRule::NorthUpBounded
            || orientation.mirror
            || orientation.stroke_reordering
            || orientation.min_degrees < -15.0
            || orientation.max_degrees > 15.0)
    {
        return Err(TemplateError::InvalidTextOrientation);
    }
    let bounds = &definition.bounds;
    if [
        bounds.min_scale,
        bounds.max_scale,
        bounds.min_aspect_ratio,
        bounds.max_aspect_ratio,
    ]
    .iter()
    .any(|value| !value.is_finite() || *value <= 0.0)
        || bounds.min_scale > bounds.max_scale
        || bounds.min_aspect_ratio > bounds.max_aspect_ratio
    {
        return Err(TemplateError::InvalidBounds);
    }
    if definition.identifying_features.is_empty() || definition.complexity.hint.trim().is_empty() {
        return Err(TemplateError::ComplexityExceeded);
    }
    Ok(())
}

fn validate_points(points: &[Point], stroke: usize) -> Result<(), TemplateError> {
    for (point, value) in points.iter().enumerate() {
        if !value.x.is_finite()
            || !value.y.is_finite()
            || !(0.0..=1.0).contains(&value.x)
            || !(0.0..=1.0).contains(&value.y)
        {
            return Err(TemplateError::NonFiniteCoordinate { stroke, point });
        }
    }
    Ok(())
}

pub fn embedded_catalogue() -> Result<Catalogue, TemplateError> {
    Catalogue::load(&[
        include_str!("../templates/heart.v1.json"),
        include_str!("../templates/hi.v1.json"),
    ])
}

pub fn render_svg(definition: &TemplateDefinition) -> String {
    fn points(values: &[Point]) -> String {
        values
            .iter()
            .map(|point| format!("{:.2},{:.2}", point.x * 100.0, point.y * 100.0))
            .collect::<Vec<_>>()
            .join(" ")
    }
    let mut svg = String::from(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\" fill=\"none\">\n",
    );
    for stroke in &definition.strokes {
        svg.push_str(&format!(
            "  <polyline class=\"stroke\" data-id=\"{}\" points=\"{}\"/>\n",
            stroke.id,
            points(&stroke.points)
        ));
    }
    for transition in &definition.transitions {
        svg.push_str(&format!(
            "  <polyline class=\"{}\" data-from=\"{}\" data-to=\"{}\" points=\"{}\"/>\n",
            transition.classification.as_str(),
            transition.from_stroke,
            transition.to_stroke,
            points(&transition.points)
        ));
    }
    svg.push_str("</svg>\n");
    svg
}
