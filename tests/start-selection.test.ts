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
  assert.deepEqual(parseManualCoordinates("28.60000, 77.20000"), {
    point: { latitude: 28.6, longitude: 77.2 },
    source: "manual",
    label: "28.60000, 77.20000",
  });
  assert.deepEqual(
    fromSearchSelection({ latitude: 28.6, longitude: 77.2 }, "Selected public entrance"),
    {
      point: { latitude: 28.6, longitude: 77.2 },
      source: "search",
      label: "Selected public entrance",
    },
  );
  assert.deepEqual(fromMapSelection({ latitude: 28.6, longitude: 77.2 }), {
    point: { latitude: 28.6, longitude: 77.2 },
    source: "map",
    label: "28.60000, 77.20000",
  });
  assert.equal(parseManualCoordinates("a nearby park"), undefined);
  assert.equal(parseManualCoordinates("91, 181"), undefined);
});

test("geolocation runs only on demand and classifies denial, timeout, unsupported, and low accuracy", async () => {
  let requests = 0;
  const successful = {
    getCurrentPosition(success: PositionCallback) {
      requests += 1;
      success({ coords: { latitude: 28.6, longitude: 77.2, accuracy: 12 } } as GeolocationPosition);
    },
  } as Geolocation;
  assert.equal(requests, 0);
  assert.deepEqual(await requestBrowserStart(successful), {
    kind: "selected",
    start: {
      point: { latitude: 28.6, longitude: 77.2 },
      source: "geolocation",
      label: "28.60000, 77.20000",
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
      success({ coords: { latitude: 28.6, longitude: 77.2, accuracy: GEOLOCATION_MAX_ACCURACY_METRES + 1 } } as GeolocationPosition);
    },
  } as Geolocation;
  assert.deepEqual(await requestBrowserStart(inaccurate), {
    kind: "low_accuracy",
    accuracyMetres: GEOLOCATION_MAX_ACCURACY_METRES + 1,
  });
});
