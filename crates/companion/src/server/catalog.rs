//! Built-in actions as the editor sees them. Settings use the same schema format as plugins.

use serde_json::{Value, json};

pub fn core_actions() -> Value {
    json!([
        { "id": "core.wait", "group": "Flow", "name": "Wait", "icon": "timer", "settings": { "type": "object", "required": ["ms"], "properties": {
            "ms": { "type": "integer", "title": "Milliseconds", "minimum": 0, "maximum": 60000, "default": 1000 } } } },
        { "id": "core.page", "group": "Deck", "name": "Go to page", "icon": "file", "settings": { "type": "object", "required": ["page"], "properties": {
            "page": { "type": "string", "title": "Page", "x-options": "pages" } } } },
        { "id": "core.profile", "group": "Deck", "name": "Switch profile", "icon": "layers", "settings": { "type": "object", "required": ["profile"], "properties": {
            "profile": { "type": "string", "title": "Profile", "x-options": "profiles" } } } },
        { "id": "core.timer", "group": "Deck", "name": "Timer", "icon": "timer", "settings": { "type": "object", "required": ["name"], "properties": {
            "name": { "type": "string", "title": "Timer name", "maxLength": 24, "default": "break" },
            "mode": { "type": "string", "title": "Type", "enum": ["countdown", "stopwatch"], "enumNames": ["Countdown", "Stopwatch"], "default": "countdown" },
            "op": { "type": "string", "title": "On press", "enum": ["toggle", "start", "stop", "reset"], "enumNames": ["Start or stop", "Start", "Stop", "Reset"], "default": "toggle" },
            "seconds": { "type": "integer", "title": "Countdown seconds", "minimum": 1, "maximum": 86400, "default": 300 } } } },
        { "id": "core.var", "group": "Deck", "name": "Set variable", "icon": "variable", "settings": { "type": "object", "required": ["name"], "properties": {
            "name": { "type": "string", "title": "Variable (starts with user.)", "maxLength": 80, "default": "user." },
            "op": { "type": "string", "title": "Change", "enum": ["set", "toggle", "inc", "dec"], "enumNames": ["Set to", "Toggle on/off", "Add", "Subtract"], "default": "set" },
            "value": { "type": "string", "title": "Value", "maxLength": 512 } } } },
        { "id": "core.keys", "group": "Keyboard", "name": "Hotkey", "icon": "keyboard", "settings": { "type": "object", "required": ["keys"], "properties": {
            "keys": { "type": "string", "title": "Keys", "format": "hotkey", "maxLength": 60, "description": "For example ctrl+shift+f13" } } } },
        { "id": "core.text", "group": "Keyboard", "name": "Type text", "icon": "type", "settings": { "type": "object", "required": ["text"], "properties": {
            "text": { "type": "string", "title": "Text", "format": "textarea", "maxLength": 2000 } } } },
        { "id": "core.media", "group": "Keyboard", "name": "Media key", "icon": "play", "settings": { "type": "object", "required": ["key"], "properties": {
            "key": { "type": "string", "title": "Key", "enum": ["play", "next", "prev", "stop", "volume-up", "volume-down", "mute"],
                     "enumNames": ["Play or pause", "Next track", "Previous track", "Stop", "Volume up", "Volume down", "Mute"], "default": "play" } } } },
        { "id": "core.open", "group": "System", "name": "Open link", "icon": "external-link", "settings": { "type": "object", "required": ["url"], "properties": {
            "url": { "type": "string", "title": "Link", "format": "url", "maxLength": 2048 } } } },
        { "id": "core.launch", "group": "System", "name": "Open app or script", "icon": "app-window", "adminOnly": true, "settings": { "type": "object", "required": ["path"], "properties": {
            "path": { "type": "string", "title": "Program", "format": "file", "maxLength": 1024, "description": "Full path to the program" },
            "args": { "type": "string", "title": "Arguments, one per line", "format": "textarea", "maxLength": 4096 } } } },
        { "id": "core.sound", "group": "Media", "name": "Play sound", "icon": "volume-2", "settings": { "type": "object", "required": ["asset"], "properties": {
            "asset": { "type": "string", "title": "Sound", "x-options": "sounds" },
            "volume": { "type": "number", "title": "Volume", "minimum": 0, "maximum": 1, "default": 1 },
            "mode": { "type": "string", "title": "Other sounds", "enum": ["mix", "stop-others"], "enumNames": ["Keep playing", "Stop them"], "default": "mix" } } } },
        { "id": "core.webhook", "group": "Network", "name": "Webhook", "icon": "webhook", "settings": { "type": "object", "required": ["url"], "properties": {
            "url": { "type": "string", "title": "URL", "format": "url", "maxLength": 4096 },
            "method": { "type": "string", "title": "Method", "enum": ["POST", "GET", "PUT", "PATCH", "DELETE"], "default": "POST" },
            "contentType": { "type": "string", "title": "Content type", "default": "application/json", "maxLength": 100 },
            "body": { "type": "string", "title": "Body", "format": "textarea", "maxLength": 65536, "description": "Use {variable} to insert values" },
            "headers": { "type": "object", "title": "Headers", "additionalProperties": { "type": "string" } } } } },
        { "id": "core.websocket", "group": "Network", "name": "WebSocket message", "icon": "radio-tower", "settings": { "type": "object", "required": ["url", "message"], "properties": {
            "url": { "type": "string", "title": "URL (ws:// or wss://)", "maxLength": 2048 },
            "message": { "type": "string", "title": "Message", "format": "textarea", "maxLength": 65536 } } } },
        { "id": "core.osc", "group": "Network", "name": "OSC message", "icon": "sliders-horizontal", "settings": { "type": "object", "required": ["host", "port", "address"], "properties": {
            "host": { "type": "string", "title": "Host", "default": "127.0.0.1", "maxLength": 255 },
            "port": { "type": "integer", "title": "Port", "minimum": 1, "maximum": 65535, "default": 9000 },
            "address": { "type": "string", "title": "Address", "default": "/", "maxLength": 255 },
            "args": { "type": "string", "title": "Arguments, one per line", "format": "textarea", "maxLength": 2000 } } } },
        { "id": "core.udp", "group": "Network", "name": "UDP message", "icon": "send", "settings": { "type": "object", "required": ["host", "port", "payload"], "properties": {
            "host": { "type": "string", "title": "Host", "default": "127.0.0.1", "maxLength": 255 },
            "port": { "type": "integer", "title": "Port", "minimum": 1, "maximum": 65535 },
            "payload": { "type": "string", "title": "Payload", "format": "textarea", "maxLength": 8192 } } } },
        { "id": "core.midi", "group": "Media", "name": "MIDI message", "icon": "piano", "settings": { "type": "object", "required": ["port", "message"], "properties": {
            "port": { "type": "string", "title": "Output", "x-options": "midi" },
            "message": { "type": "string", "title": "Message", "enum": ["note-on", "note-off", "cc", "program"], "enumNames": ["Note on", "Note off", "Control change", "Program change"], "default": "note-on" },
            "channel": { "type": "integer", "title": "Channel", "minimum": 1, "maximum": 16, "default": 1 },
            "data1": { "type": "integer", "title": "Note or controller", "minimum": 0, "maximum": 127, "default": 60 },
            "data2": { "type": "integer", "title": "Velocity or value", "minimum": 0, "maximum": 127, "default": 127 } } } }
    ])
}

/// Schema for one core action, used to clean settings before they are stored.
pub fn core_schema(kind: &str) -> Option<crate::plugins::manifest::Schema> {
    core_actions().as_array()?.iter().find(|a| a["id"] == kind).and_then(|a| serde_json::from_value(a["settings"].clone()).ok())
}
