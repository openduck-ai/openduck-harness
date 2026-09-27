# openduck-context-management

Conversation compaction and tiered context management for OpenDuck.

## Layers

1. **`snip` (Tier 1: Zero-cost static string compression)**
   - Truncates oversized tool responses (e.g. search dumps, long compiler logs) with structured omission annotations.
   - Identifies protected zones (`is_protected_zone_message`) to safeguard user instructions and critical task state.
2. **`summarize` & `structured` (Tier 3: LLM-driven structured summary)**
   - Extracts structured facts (user intent, modified files, technical concepts, error resolutions, next steps).
3. **`compact` (Conversation Compactor)**
   - Trait-based compaction engine (`CompactionInput` / `CompactionOutput`) for whole-session summarization.

## Usage

```rust
use openduck_context_management::{snip_tool_output, is_protected_zone_message};

// Tier 1 static Snip
let (snipped_text, was_snipped) = snip_tool_output(&raw_output, 60, 12 * 1024);

// Protected zone check
if is_protected_zone_message(&message) {
    // Retain message without compression
}
```
