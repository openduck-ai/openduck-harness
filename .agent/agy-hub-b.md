# Subtask B: Hub UI role-level provider+model selection

Read `.agent/plan-hub-role-model-ui.md` fully and implement it. You own ONLY:
- ui/hub-core/src/types.ts
- ui/hub-core/src/rest.ts
- ui/hub-core/src/rest.test.ts
- ui/hub/src/providerConfig.ts
- ui/hub/src/providerConfig.test.ts
- ui/hub/src/components/OpenDuckConfigPanel.tsx

Requirements (exact names/anchors are in the plan file):
1. Role selection block above the provider table (Major provider/model, Secondary provider/model selects).
2. Provider candidates: enabled && configured (no non-empty model requirement); secondary must not exclude major.
3. Model candidates from GET models[] per provider; searchable; recommendation flags only affect ordering; empty list or
   load failure => manual model-ID input.
4. Switching provider clears the old model draft; "inherit default (xxx)" option when a default exists, else require
   explicit choice.
5. Effective model not in models[] still displayed, marked custom/unlisted.
6. Per-provider refresh button: POST /api/v1/config/providers/{id}/models/refresh (202, empty body), then re-read GET;
   handle loading/empty/failure-with-retry; attribute refresh results by providerId so a stale response never overwrites
   a provider the user switched away from.
7. fromEnv=true => show file value and effective value, lock editing, never write env-effective value back.
8. Rename table model column to "Default model (optional)"; move fast_model/secondary_model into a collapsible
   advanced section.
9. After save, rebuild draft from the server response and clear dirty.
10. Old server (GET without selectionVersion) => legacy single-model editing mode with a notice, never send v2.
11. TypeScript strict, no `any` escapes, no new dependencies, never touch ui/desktop/src/api.

Then run the tests/typecheck for both packages (check ui/hub and ui/hub-core package.json scripts) and iterate until
green.

Report: files+functions changed, requirement->implementation mapping, test output summary lines.
