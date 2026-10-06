use gpsart_persistence::routes::CandidateSnapshot;

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn filename_part(value: &str) -> String {
    let mut sanitized = String::new();
    let mut separator = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            if separator && !sanitized.is_empty() && sanitized.len() < 40 {
                sanitized.push('-');
            }
            separator = false;
            if sanitized.len() < 40 {
                sanitized.push(character);
            }
        } else {
            separator = true;
        }
    }
    while sanitized.ends_with('-') {
        sanitized.pop();
    }
    if sanitized.is_empty() {
        "route".to_owned()
    } else {
        sanitized
    }
}

pub(crate) fn filename(snapshot: &CandidateSnapshot) -> String {
    format!(
        "gpsart-{}-{}-v{}.gpx",
        filename_part(&snapshot.versions.template_id),
        snapshot.id,
        snapshot.candidate_version
    )
}

pub(crate) fn serialize(snapshot: &CandidateSnapshot) -> String {
    let route_id = xml(&snapshot.id.to_string());
    let points = snapshot
        .geometry
        .coordinates
        .iter()
        .map(|[longitude, latitude]| {
            format!("      <trkpt lat=\"{latitude}\" lon=\"{longitude}\"></trkpt>")
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gpx version=\"1.1\" creator=\"GPSART\" xmlns=\"http://www.topografix.com/GPX/1/1\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:schemaLocation=\"http://www.topografix.com/GPX/1/1 https://www.topografix.com/GPX/1/1/gpx.xsd\">\n  <metadata>\n    <name>GPSART planned route {route_id}</name>\n    <desc>Planned route from immutable snapshot route-candidate-snapshot-v1; not a recorded activity.</desc>\n  </metadata>\n  <trk>\n    <name>GPSART {}@{} planned route</name>\n    <desc>Exact ordered traversal including connectors and retraced sections.</desc>\n    <src>GPSART route {route_id}; candidate version {}; profile {}; routing data {}</src>\n    <type>planned-route</type>\n    <trkseg>\n{points}\n    </trkseg>\n  </trk>\n</gpx>\n",
        xml(&snapshot.versions.template_id),
        snapshot.versions.template,
        snapshot.candidate_version,
        xml(&snapshot.versions.profile),
        xml(&snapshot.versions.routing_data)
    )
}

#[cfg(test)]
mod tests {
    use super::{filename_part, xml};

    #[test]
    fn metadata_and_filename_components_are_safe() {
        assert_eq!(
            filename_part("HI & <script> / \"route\""),
            "hi-script-route"
        );
        assert_eq!(filename_part("../../"), "route");
        assert_eq!(xml("<&\"'>"), "&lt;&amp;&quot;&apos;&gt;");
    }
}
