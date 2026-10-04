# Bengaluru pilot OSM snapshot

`cubbon-park-osm-v1.json` is a deliberately small, public-data routing snapshot for the TS-105/TS-106 feasibility gates. It contains 40 versioned OpenStreetMap nodes and five versioned ways from the Cubbon Park pilot area. It is not a Bengaluru coverage extract and must not be presented as one.

The source was fetched once from the OpenStreetMap API 0.6 bounding-box endpoint recorded in the JSON. The raw response SHA-256 was `3828e4099d571653e7cd1ee05caa73dadb5ec34ad99b80906cc24a0be3a5c5da`. The committed derived artifact SHA-256 is stored beside it. Every retained node and way includes its immutable OSM element version and timestamp; contributor user names and IDs are intentionally omitted.

The extraction script accepts only the exact v1 raw bytes, fails if a relation touches a retained way, and emits the snapshot plus checksum:

```powershell
node scripts/extract-osm-pilot-snapshot.mjs <raw-map.json> crates/gpsart-routing/data/cubbon-park-osm-v1.json crates/gpsart-routing/data/cubbon-park-osm-v1.sha256
```

The original API permits bounding-box downloads and version-addressed element reads. Data is © OpenStreetMap contributors and licensed under ODbL 1.0. See the [OpenStreetMap API 0.6 documentation](https://wiki.openstreetmap.org/wiki/API_v0.6) and [copyright and licence page](https://www.openstreetmap.org/copyright).
