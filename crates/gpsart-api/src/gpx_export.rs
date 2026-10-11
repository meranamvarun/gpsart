use gpsart_persistence::routes::CandidateSnapshot;

fn xml(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if matches!(character, '\u{9}' | '\u{a}' | '\u{d}')
                || matches!(character as u32, 0x20..=0xd7ff | 0xe000..=0xfffd | 0x10000..=0x10ffff)
            {
                character
            } else {
                '\u{fffd}'
            }
        })
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn coordinate(value: f64) -> String {
    value.to_string()
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
    let copyright = snapshot
        .provenance
        .attribution
        .as_deref()
        .map(|attribution| {
            format!(
                "    <copyright author=\"{}\"></copyright>\n",
                xml(attribution)
            )
        })
        .unwrap_or_default();
    let points = snapshot
        .geometry
        .coordinates
        .iter()
        .map(|[longitude, latitude]| {
            format!(
                "      <trkpt lat=\"{}\" lon=\"{}\"></trkpt>",
                coordinate(*latitude),
                coordinate(*longitude)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gpx version=\"1.1\" creator=\"GPSART\" xmlns=\"http://www.topografix.com/GPX/1/1\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:schemaLocation=\"http://www.topografix.com/GPX/1/1 https://www.topografix.com/GPX/1/1/gpx.xsd\">\n  <metadata>\n    <name>GPSART planned route {route_id}</name>\n    <desc>Planned route from immutable snapshot route-candidate-snapshot-v1; not a recorded activity.</desc>\n{copyright}  </metadata>\n  <trk>\n    <name>GPSART {}@{} planned route</name>\n    <desc>Exact ordered traversal including connectors and retraced sections.</desc>\n    <src>GPSART route {route_id}; candidate version {}; profile {}; routing data {}</src>\n    <type>planned-route</type>\n    <trkseg>\n{points}\n    </trkseg>\n  </trk>\n</gpx>\n",
        xml(&snapshot.versions.template_id),
        snapshot.versions.template,
        snapshot.candidate_version,
        xml(&snapshot.versions.profile),
        xml(&snapshot.versions.routing_data)
    )
}

#[cfg(test)]
mod tests {
    use super::{coordinate, filename_part, xml};

    #[test]
    fn metadata_and_filename_components_are_safe() {
        assert_eq!(
            filename_part("HI & <script> / \"route\""),
            "hi-script-route"
        );
        assert_eq!(filename_part("../../"), "route");
        assert_eq!(xml("<&\"'>\0\u{b}"), "&lt;&amp;&quot;&apos;&gt;��");
        assert_eq!(coordinate(0.000_000_1), "0.0000001");
        assert_eq!(coordinate(-0.000_000_1), "-0.0000001");
    }

    #[test]
    fn canonical_coordinate_serialization_round_trips_without_exponents() {
        for encoded in [
            -1_800_000_000_i32,
            -900_000_000,
            -1,
            0,
            1,
            900_000_000,
            1_800_000_000,
        ] {
            let value = f64::from(encoded) / 10_000_000.0;
            let rendered = coordinate(value);
            assert!(!rendered.contains(['e', 'E']));
            assert_eq!(rendered.parse::<f64>().unwrap(), value);
        }
    }
}
