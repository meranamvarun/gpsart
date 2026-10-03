import assert from "node:assert/strict";
import test from "node:test";

import {
  GEOLOCATION_MAX_ACCURACY_METRES,
  fromMapSelection,
  fromSearchSelection,
  parseManualCoordinates,
  requestBrowserStart,
} from "../src/domain/start-selection.ts";

test("manual, search, and map inputs produce one proposed-start model", () => {
  assert.deepEqual(parseManualCoordinates("0.50000, -150.25000"), {
    point: { latitude: 0.5, longitude: -150.25 },
    source: "manual",
    label: "0.50000, -150.25000",
  });
  assert.deepEqual(
    fromSearchSelection({ latitude: 0.5, longitude: -150.25 }, "Synthetic ocean test point"),
    {
      point: { latitude: 0.5, longitude: -150.25 },
      source: "search",
      label: "Synthetic ocean test point",
    },
  );
  assert.deepEqual(fromMapSelection({ latitude: 0.5, longitude: -150.25 }), {
    point: { latitude: 0.5, longitude: -150.25 },
    source: "map",
    label: "0.50000, -150.25000",
  });
  assert.equal(parseManualCoordinates("a nearby park"), undefined);
  assert.equal(parseManualCoordinates("91, 181"), undefined);
});

test("geolocation runs only on demand and classifies denial, timeout, unsupported, and low accuracy", async () => {
  let requests = 0;
  const successful = {
    getCurrentPosition(success: PositionCallback) {
      requests += 1;
      success({ coords: { latitude: 0.5, longitude: -150.25, accuracy: 12 } } as GeolocationPosition);
    },
  } as Geolocation;
  assert.equal(requests, 0);
  assert.deepEqual(await requestBrowserStart(successful), {
    kind: "selected",
    start: {
      point: { latitude: 0.5, longitude: -150.25 },
      source: "geolocation",
      label: "0.50000, -150.25000",
      accuracyMetres: 12,
    },
  });
  assert.equal(requests, 1);

  assert.deepEqual(await requestBrowserStart(undefined), { kind: "unsupported" });
  for (const [code, kind] of [[1, "denied"], [2, "unavailable"], [3, "timeout"]] as const) {
    const geolocation = {
      getCurrentPosition(_success: PositionCallback, error: PositionErrorCallback) {
        error({ code } as GeolocationPositionError);
      },
    } as Geolocation;
    assert.deepEqual(await requestBrowserStart(geolocation), { kind });
  }
  const inaccurate = {
    getCurrentPosition(success: PositionCallback) {
      success({ coords: { latitude: 0.5, longitude: -150.25, accuracy: GEOLOCATION_MAX_ACCURACY_METRES + 1 } } as GeolocationPosition);
    },
  } as Geolocation;
  assert.deepEqual(await requestBrowserStart(inaccurate), {
    kind: "low_accuracy",
    accuracyMetres: GEOLOCATION_MAX_ACCURACY_METRES + 1,
  });
});
