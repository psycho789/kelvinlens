// Wire fixtures: one sample of every type that crosses the Rust/TypeScript boundary.
//
// `npm run typecheck` checks each sample against its TypeScript type; the shell test
// `wire_tests` (src-tauri/src/wire_tests.rs) reads this file and compares each sample with what
// serde writes or accepts. Rules: strict JSON inside each constant (quoted keys, no comments),
// numbers that are exact in f32 (multiples of 0.25), every optional field filled unless the
// fixture is about the empty case. Limit: types Rust only reads (ReportItem, TriageSettings,
// MapItem) cannot reveal an extra TypeScript field, because serde drops unknown fields.
import type { Annotation, Camera, CmdError, Finding, Formula, FormulaResult, Fusion, ImageInfo, InspectSettings, Look, MapItem, Measurement, Params, Presentation, ProfilePoint, Recomputed, ReportInfo, ReportItem, Series, Shape, Sidecar, Stats, StitchResult, Template, TriageRow, TriageSettings, VideoInfo } from "./api";

export const params = {
  "emissivity": 0.875,
  "reflectedK": 293.25,
  "atmosphereK": 295.5,
  "distanceM": 2.5,
  "humidity": 0.5,
  "windowK": 290.0,
  "windowTransmission": 0.75
} satisfies Params;

export const shapes = [
  {
    "kind": "spot",
    "x": 1.5,
    "y": 2.5
  },
  {
    "kind": "box",
    "x": 1.0,
    "y": 2.0,
    "w": 3.0,
    "h": 4.0
  },
  {
    "kind": "ellipse",
    "cx": 5.0,
    "cy": 6.0,
    "rx": 2.5,
    "ry": 1.5
  },
  {
    "kind": "line",
    "x1": 0.0,
    "y1": 1.0,
    "x2": 8.0,
    "y2": 9.0
  },
  {
    "kind": "polygon",
    "points": [
      [
        0.0,
        0.0
      ],
      [
        4.0,
        0.0
      ],
      [
        2.0,
        3.0
      ]
    ]
  },
  {
    "kind": "polyline",
    "points": [
      [
        0.0,
        0.0
      ],
      [
        1.5,
        2.5
      ],
      [
        3.0,
        1.0
      ]
    ]
  },
  {
    "kind": "wand",
    "x": 3.0,
    "y": 4.0,
    "tolerance": 1.5
  }
] satisfies Shape[];

export const measurement = {
  "label": "M1",
  "shape": {
    "kind": "box",
    "x": 1.0,
    "y": 2.0,
    "w": 3.0,
    "h": 4.0
  },
  "alarm": {
    "stat": "max",
    "above": 330.0,
    "below": 280.0
  }
} satisfies Measurement;

export const stats = {
  "min": 290.5,
  "max": 320.25,
  "avg": 300.0,
  "stdDev": 2.0,
  "minAt": [
    1,
    2
  ],
  "maxAt": [
    3,
    4
  ],
  "count": 9
} satisfies Stats;

export const look = {
  "palette": {
    "id": "test",
    "name": "Test",
    "colors": [
      [
        0,
        0,
        0
      ],
      [
        128,
        64,
        32
      ],
      [
        255,
        255,
        255
      ]
    ]
  },
  "range": [
    290.0,
    320.0
  ],
  "distribution": "detail",
  "isotherms": [
    {
      "kind": "above",
      "t": 310.0,
      "color": [
        255,
        0,
        0
      ],
      "opacity": 0.5
    },
    {
      "kind": "below",
      "t": 280.0,
      "color": [
        0,
        0,
        255
      ],
      "opacity": 1.0
    },
    {
      "kind": "interval",
      "lo": 290.0,
      "hi": 300.0,
      "color": [
        0,
        255,
        0
      ],
      "opacity": 0.75
    },
    {
      "kind": "condensation",
      "airK": 295.0,
      "rh": 0.5,
      "limit": 0.25,
      "color": [
        0,
        255,
        255
      ],
      "opacity": 0.5
    },
    {
      "kind": "insulation",
      "insideK": 295.0,
      "outsideK": 275.0,
      "index": 0.5,
      "color": [
        255,
        255,
        0
      ],
      "opacity": 0.25
    }
  ]
} satisfies Look;

export const fusions = [
  {
    "mode": "msx",
    "strength": 0.5
  },
  {
    "mode": "interval",
    "lo": 300.0,
    "hi": 310.0
  }
] satisfies Fusion[];

export const formula = {
  "label": "Delta",
  "expression": "M1.max - M1.min",
  "kind": "difference"
} satisfies Formula;

export const formulaResults = [
  {
    "value": 1.5,
    "error": null
  },
  {
    "value": null,
    "error": "Unknown name"
  }
] satisfies FormulaResult[];

export const annotations = [
  {
    "kind": "text",
    "x": 1.0,
    "y": 2.0,
    "text": "Check"
  },
  {
    "kind": "sketch",
    "points": [
      [
        0.0,
        0.0
      ],
      [
        1.5,
        2.5
      ]
    ],
    "color": [
      255,
      0,
      0
    ]
  }
] satisfies Annotation[];

export const presentation = {
  "rotation": 90.0,
  "crop": [
    1.0,
    2.0,
    30.0,
    40.0
  ],
  "align": [
    1.5,
    -2.0,
    1.25
  ]
} satisfies Presentation;

export const inspectSettings = {
  "mode": "both",
  "referenceK": 300.0,
  "basis": "ambient",
  "minDelta": 2.0,
  "minPixels": 6,
  "maxFindings": 8,
  "insideK": 295.0,
  "outsideK": 275.0
} satisfies InspectSettings;

export const finding = {
  "label": "A1",
  "hot": true,
  "area": {
    "kind": "box",
    "x": 2.0,
    "y": 2.0,
    "w": 3.0,
    "h": 3.0
  },
  "pixels": 9,
  "extremeK": 325.0,
  "at": [
    3,
    3
  ],
  "referenceK": 300.0,
  "deltaK": 25.0,
  "thermalIndex": 0.5,
  "severity": {
    "level": 4,
    "label": "Major discrepancy",
    "action": "Repair immediately"
  }
} satisfies Finding;

export const sidecar = {
  "version": 2,
  "params": {
    "emissivity": 0.875,
    "reflectedK": 293.25,
    "atmosphereK": 295.5,
    "distanceM": 2.5,
    "humidity": 0.5,
    "windowK": 290.0,
    "windowTransmission": 0.75
  },
  "measurements": [
    {
      "label": "M1",
      "shape": {
        "kind": "box",
        "x": 1.0,
        "y": 2.0,
        "w": 3.0,
        "h": 4.0
      },
      "alarm": {
        "stat": "max",
        "above": 330.0,
        "below": 280.0
      }
    }
  ],
  "notes": "Notes",
  "palette": "iron",
  "range": [
    290.0,
    320.0
  ],
  "distribution": "histogram",
  "isotherms": [
    {
      "kind": "above",
      "t": 310.0,
      "color": [
        255,
        0,
        0
      ],
      "opacity": 0.5
    }
  ],
  "formulas": [
    {
      "label": "Delta",
      "expression": "M1.max - M1.min",
      "kind": "difference"
    }
  ],
  "annotations": [
    {
      "kind": "text",
      "x": 1.0,
      "y": 2.0,
      "text": "Check"
    },
    {
      "kind": "sketch",
      "points": [
        [
          0.0,
          0.0
        ],
        [
          1.5,
          2.5
        ]
      ],
      "color": [
        255,
        0,
        0
      ]
    }
  ],
  "findings": [
    {
      "label": "A1",
      "hot": true,
      "area": {
        "kind": "box",
        "x": 2.0,
        "y": 2.0,
        "w": 3.0,
        "h": 3.0
      },
      "pixels": 9,
      "extremeK": 325.0,
      "at": [
        3,
        3
      ],
      "referenceK": 300.0,
      "deltaK": 25.0,
      "thermalIndex": 0.5,
      "severity": {
        "level": 4,
        "label": "Major discrepancy",
        "action": "Repair immediately"
      }
    }
  ],
  "asset": "Pump 3",
  "fields": {
    "Component": "Breaker"
  },
  "presentation": {
    "rotation": 90.0,
    "crop": [
      1.0,
      2.0,
      30.0,
      40.0
    ],
    "align": [
      1.5,
      -2.0,
      1.25
    ]
  }
} satisfies Sidecar;

export const camera = {
  "description": "Camera",
  "serialNumber": "SN1",
  "dateTime": "2026-10-19T10:00:00",
  "latitude": 51.5,
  "longitude": -0.25,
  "altitude": 12.5,
  "heading": 90.0,
  "lens": "24 deg",
  "fieldOfView": 24.0,
  "focalLength": 13.5,
  "gimbal": [
    1.0,
    2.0,
    3.0
  ],
  "relativeAltitude": 10.5
} satisfies Camera;

export const videoInfo = {
  "frames": 3,
  "width": 4,
  "height": 2,
  "compressed": false,
  "times": [
    0.0,
    0.25,
    0.5
  ],
  "frameRate": 4.0
} satisfies VideoInfo;

export const imageInfo = {
  "id": 1,
  "path": "/data/a.jpg",
  "name": "a.jpg",
  "format": "flir",
  "formatLabel": "FLIR radiometric JPEG",
  "width": 4,
  "height": 2,
  "params": {
    "emissivity": 0.875,
    "reflectedK": 293.25,
    "atmosphereK": 295.5,
    "distanceM": 2.5,
    "humidity": 0.5,
    "windowK": 290.0,
    "windowTransmission": 0.75
  },
  "fileParams": {
    "emissivity": 0.875,
    "reflectedK": 293.25,
    "atmosphereK": 295.5,
    "distanceM": 2.5,
    "humidity": 0.5,
    "windowK": 290.0,
    "windowTransmission": 0.75
  },
  "editable": [
    "emissivity",
    "reflectedK"
  ],
  "camera": {
    "description": "Camera",
    "serialNumber": "SN1",
    "dateTime": "2026-10-19T10:00:00",
    "latitude": 51.5,
    "longitude": -0.25,
    "altitude": 12.5,
    "heading": 90.0,
    "lens": "24 deg",
    "fieldOfView": 24.0,
    "focalLength": 13.5,
    "gimbal": [
      1.0,
      2.0,
      3.0
    ],
    "relativeAltitude": 10.5
  },
  "embeddedMeasurements": [
    {
      "label": "M1",
      "shape": {
        "kind": "box",
        "x": 1.0,
        "y": 2.0,
        "w": 3.0,
        "h": 4.0
      },
      "alarm": {
        "stat": "max",
        "above": 330.0,
        "below": 280.0
      }
    }
  ],
  "embeddedRange": [
    280.0,
    330.0
  ],
  "embeddedPalette": [
    [
      0,
      0,
      0
    ],
    [
      255,
      255,
      255
    ]
  ],
  "range": [
    290.0,
    320.0
  ],
  "photoSize": [
    640,
    480
  ],
  "pip": {
    "source": {
      "x": 0,
      "y": 0,
      "w": 80,
      "h": 60
    },
    "destination": {
      "x": 10,
      "y": -20,
      "w": 160,
      "h": 120
    }
  },
  "flirJpeg": true,
  "video": {
    "frames": 3,
    "width": 4,
    "height": 2,
    "compressed": false,
    "times": [
      0.0,
      0.25,
      0.5
    ],
    "frameRate": 4.0
  },
  "sidecar": {
    "version": 2,
    "params": {
      "emissivity": 0.875,
      "reflectedK": 293.25,
      "atmosphereK": 295.5,
      "distanceM": 2.5,
      "humidity": 0.5,
      "windowK": 290.0,
      "windowTransmission": 0.75
    },
    "measurements": [
      {
        "label": "M1",
        "shape": {
          "kind": "box",
          "x": 1.0,
          "y": 2.0,
          "w": 3.0,
          "h": 4.0
        },
        "alarm": {
          "stat": "max",
          "above": 330.0,
          "below": 280.0
        }
      }
    ],
    "notes": "Notes",
    "palette": "iron",
    "range": [
      290.0,
      320.0
    ],
    "distribution": "histogram",
    "isotherms": [
      {
        "kind": "above",
        "t": 310.0,
        "color": [
          255,
          0,
          0
        ],
        "opacity": 0.5
      }
    ],
    "formulas": [
      {
        "label": "Delta",
        "expression": "M1.max - M1.min",
        "kind": "difference"
      }
    ],
    "annotations": [
      {
        "kind": "text",
        "x": 1.0,
        "y": 2.0,
        "text": "Check"
      },
      {
        "kind": "sketch",
        "points": [
          [
            0.0,
            0.0
          ],
          [
            1.5,
            2.5
          ]
        ],
        "color": [
          255,
          0,
          0
        ]
      }
    ],
    "findings": [
      {
        "label": "A1",
        "hot": true,
        "area": {
          "kind": "box",
          "x": 2.0,
          "y": 2.0,
          "w": 3.0,
          "h": 3.0
        },
        "pixels": 9,
        "extremeK": 325.0,
        "at": [
          3,
          3
        ],
        "referenceK": 300.0,
        "deltaK": 25.0,
        "thermalIndex": 0.5,
        "severity": {
          "level": 4,
          "label": "Major discrepancy",
          "action": "Repair immediately"
        }
      }
    ],
    "asset": "Pump 3",
    "fields": {
      "Component": "Breaker"
    },
    "presentation": {
      "rotation": 90.0,
      "crop": [
        1.0,
        2.0,
        30.0,
        40.0
      ],
      "align": [
        1.5,
        -2.0,
        1.25
      ]
    }
  },
  "sidecarError": "bad edits file"
} satisfies ImageInfo;

export const recomputed = {
  "params": {
    "emissivity": 0.875,
    "reflectedK": 293.25,
    "atmosphereK": 295.5,
    "distanceM": 2.5,
    "humidity": 0.5,
    "windowK": 290.0,
    "windowTransmission": 0.75
  },
  "range": [
    290.0,
    320.0
  ]
} satisfies Recomputed;

export const profilePoint = {
  "d": 1.5,
  "k": 300.25
} satisfies ProfilePoint;

export const triageRows = [
  {
    "path": "/data/a.jpg",
    "name": "a.jpg",
    "error": null,
    "minK": 290.5,
    "maxK": 331.25,
    "flagged": true,
    "dateTime": "2026-10-19T10:00:00"
  },
  {
    "path": "/data/b.jpg",
    "name": "b.jpg",
    "error": "Could not open thermogram: bad",
    "minK": null,
    "maxK": null,
    "flagged": false,
    "dateTime": null
  }
] satisfies TriageRow[];

export const stitchResult = {
  "info": {
    "id": 2,
    "path": "/data/pano.png",
    "name": "pano.png",
    "format": "panorama",
    "formatLabel": "Panorama",
    "width": 8,
    "height": 2,
    "params": {
      "emissivity": 0.875,
      "reflectedK": 293.25,
      "atmosphereK": 295.5,
      "distanceM": 2.5,
      "humidity": 0.5,
      "windowK": 290.0,
      "windowTransmission": 0.75
    },
    "fileParams": {
      "emissivity": 0.875,
      "reflectedK": 293.25,
      "atmosphereK": 295.5,
      "distanceM": 2.5,
      "humidity": 0.5,
      "windowK": 290.0,
      "windowTransmission": 0.75
    },
    "editable": [],
    "camera": {
      "description": null,
      "serialNumber": null,
      "dateTime": null,
      "latitude": null,
      "longitude": null,
      "altitude": null,
      "heading": null,
      "lens": null,
      "fieldOfView": null,
      "focalLength": null,
      "gimbal": null,
      "relativeAltitude": null
    },
    "embeddedMeasurements": [],
    "embeddedRange": null,
    "embeddedPalette": null,
    "range": null,
    "photoSize": null,
    "pip": null,
    "flirJpeg": false,
    "video": null,
    "sidecar": null,
    "sidecarError": null
  },
  "offsets": [
    [
      0,
      0
    ],
    [
      10,
      -2
    ]
  ],
  "scores": [
    0.75,
    0.5
  ]
} satisfies StitchResult;

export const series = {
  "rows": [
    [
      {
        "min": 290.5,
        "max": 320.25,
        "avg": 300.0,
        "stdDev": 2.0,
        "minAt": [
          1,
          2
        ],
        "maxAt": [
          3,
          4
        ],
        "count": 9
      },
      null
    ],
    [
      {
        "min": 290.5,
        "max": 320.25,
        "avg": 300.0,
        "stdDev": 2.0,
        "minAt": [
          1,
          2
        ],
        "maxAt": [
          3,
          4
        ],
        "count": 9
      },
      {
        "min": 290.5,
        "max": 320.25,
        "avg": 300.0,
        "stdDev": 2.0,
        "minAt": [
          1,
          2
        ],
        "maxAt": [
          3,
          4
        ],
        "count": 9
      }
    ]
  ],
  "times": [
    0.0,
    0.5
  ]
} satisfies Series;

export const template = {
  "id": "custom",
  "name": "Custom",
  "kind": "electrical",
  "company": "Acme",
  "accent": "#c2410c",
  "logo": "data:image/png;base64,AAAA",
  "coverFields": [
    "Customer"
  ],
  "imageFields": [
    "Component"
  ],
  "sections": {
    "summary": true,
    "contents": false,
    "findingsOverview": true,
    "trends": false,
    "signature": true
  },
  "imageSections": [
    "fields",
    "findings",
    "notes"
  ],
  "imagesPerPage": 2,
  "disclaimer": "Disclaimer",
  "footer": "Footer"
} satisfies Template;

export const reportInfo = {
  "title": "Survey",
  "site": "Plant 1",
  "inspector": "A. Person",
  "date": "2026-10-19",
  "summary": "All fine",
  "paper": "us-letter",
  "unit": "F",
  "includePhotos": true,
  "template": {
    "id": "custom",
    "name": "Custom",
    "kind": "electrical",
    "company": "Acme",
    "accent": "#c2410c",
    "logo": "data:image/png;base64,AAAA",
    "coverFields": [
      "Customer"
    ],
    "imageFields": [
      "Component"
    ],
    "sections": {
      "summary": true,
      "contents": false,
      "findingsOverview": true,
      "trends": false,
      "signature": true
    },
    "imageSections": [
      "fields",
      "findings",
      "notes"
    ],
    "imagesPerPage": 2,
    "disclaimer": "Disclaimer",
    "footer": "Footer"
  },
  "fields": {
    "Customer": "Acme"
  },
  "conditions": {
    "insideK": 295.0,
    "outsideK": 275.0,
    "humidity": 0.5,
    "ambientK": 293.0,
    "basis": "ambient"
  },
  "format": "html"
} satisfies ReportInfo;

export const reportItem = {
  "id": 1,
  "look": {
    "palette": {
      "id": "test",
      "name": "Test",
      "colors": [
        [
          0,
          0,
          0
        ],
        [
          128,
          64,
          32
        ],
        [
          255,
          255,
          255
        ]
      ]
    },
    "range": [
      290.0,
      320.0
    ],
    "distribution": "detail",
    "isotherms": [
      {
        "kind": "above",
        "t": 310.0,
        "color": [
          255,
          0,
          0
        ],
        "opacity": 0.5
      },
      {
        "kind": "below",
        "t": 280.0,
        "color": [
          0,
          0,
          255
        ],
        "opacity": 1.0
      },
      {
        "kind": "interval",
        "lo": 290.0,
        "hi": 300.0,
        "color": [
          0,
          255,
          0
        ],
        "opacity": 0.75
      },
      {
        "kind": "condensation",
        "airK": 295.0,
        "rh": 0.5,
        "limit": 0.25,
        "color": [
          0,
          255,
          255
        ],
        "opacity": 0.5
      },
      {
        "kind": "insulation",
        "insideK": 295.0,
        "outsideK": 275.0,
        "index": 0.5,
        "color": [
          255,
          255,
          0
        ],
        "opacity": 0.25
      }
    ]
  },
  "measurements": [
    {
      "label": "M1",
      "shape": {
        "kind": "box",
        "x": 1.0,
        "y": 2.0,
        "w": 3.0,
        "h": 4.0
      },
      "alarm": {
        "stat": "max",
        "above": 330.0,
        "below": 280.0
      }
    }
  ],
  "notes": "Notes",
  "formulas": [
    {
      "label": "Delta",
      "expression": "M1.max - M1.min",
      "kind": "difference"
    }
  ],
  "annotations": [
    {
      "kind": "text",
      "x": 1.0,
      "y": 2.0,
      "text": "Check"
    },
    {
      "kind": "sketch",
      "points": [
        [
          0.0,
          0.0
        ],
        [
          1.5,
          2.5
        ]
      ],
      "color": [
        255,
        0,
        0
      ]
    }
  ],
  "findings": [
    {
      "label": "A1",
      "hot": true,
      "area": {
        "kind": "box",
        "x": 2.0,
        "y": 2.0,
        "w": 3.0,
        "h": 3.0
      },
      "pixels": 9,
      "extremeK": 325.0,
      "at": [
        3,
        3
      ],
      "referenceK": 300.0,
      "deltaK": 25.0,
      "thermalIndex": 0.5,
      "severity": {
        "level": 4,
        "label": "Major discrepancy",
        "action": "Repair immediately"
      }
    }
  ],
  "fields": {
    "Component": "Breaker"
  },
  "asset": "Pump 3",
  "presentation": {
    "rotation": 90.0,
    "crop": [
      1.0,
      2.0,
      30.0,
      40.0
    ],
    "align": [
      1.5,
      -2.0,
      1.25
    ]
  }
} satisfies ReportItem;

export const triageSettings = {
  "aboveK": 330.0,
  "belowK": 270.0
} satisfies TriageSettings;

export const mapItem = {
  "id": 1,
  "description": "North wall"
} satisfies MapItem;

export const cmdError = {
  "kind": "notFound",
  "message": "This image is no longer open"
} satisfies CmdError;
