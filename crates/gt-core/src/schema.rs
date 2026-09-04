pub const AUTHORING_SCHEMA_JSON: &str = r###"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://github.com/MikanseiLaboratory/gt-parser/docs/authoring.schema.json",
  "title": "GT Title authoring IR",
  "type": "object",
  "required": ["width", "height", "layers"],
  "properties": {
    "width": { "type": "number" },
    "height": { "type": "number" },
    "layers": {
      "type": "array",
      "items": { "$ref": "#/$defs/layer" }
    },
    "storyboards": {
      "type": "array",
      "items": { "$ref": "#/$defs/storyboard" }
    }
  },
  "$defs": {
    "layer": {
      "type": "object",
      "required": ["name", "objects"],
      "properties": {
        "name": { "type": "string" },
        "location": { "$ref": "#/$defs/vec3" },
        "dimensions": { "$ref": "#/$defs/vec3" },
        "objects": {
          "type": "array",
          "items": { "$ref": "#/$defs/layerChild" }
        }
      }
    },
    "layerChild": {
      "type": "object",
      "properties": {
        "node": { "enum": ["object", "layer"] }
      }
    },
    "storyboard": {
      "type": "object",
      "properties": {
        "storyboard_type": { "type": ["string", "null"] },
        "data_name": { "type": ["string", "null"] },
        "animations": {
          "type": "array",
          "items": { "$ref": "#/$defs/animation" }
        }
      }
    },
    "animation": {
      "type": "object",
      "required": ["kind"],
      "properties": {
        "kind": { "type": "string" },
        "object": { "type": ["string", "null"] },
        "duration": { "type": ["string", "null"] },
        "delay": { "type": ["string", "null"] },
        "interpolation": { "type": ["string", "null"] },
        "direction": { "type": ["string", "null"] },
        "reversed": { "type": "boolean" },
        "center_axis": { "type": ["string", "null"] },
        "speed": { "type": ["string", "null"] }
      }
    },
    "vec3": {
      "type": "object",
      "properties": {
        "x": { "type": "number" },
        "y": { "type": "number" },
        "z": { "type": "number" }
      }
    }
  }
}
"###;

pub const FORMAT_SUMMARY: &str = r#"GTZIP is a ZIP containing document.xml (UTF-8 or UTF-16), resources.xml, [Content_Types].xml, and GUID-named asset blobs.
document.xml root is Composition. Layers contain objects (TextBlock, Image, Rectangle, Ellipse, Ticker). Storyboards are siblings after layers.
Brush uses Type=Solid|LinearGradient|RadialGradient|Bitmap. Location is the Anchor point; IR stores top-left after parse.
Image sequences are one resource with many source elements. Do not collapse them to a still.
Author via IR JSON, then pack to GTZIP. Do not hand-write document.xml encoding or GUIDs.
"#;
