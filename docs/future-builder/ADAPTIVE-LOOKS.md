# Adaptive looks

Implemented locally for 0.5. **Setups → Adaptive looks** offers Tidal, Paper and
Moss in Day, Evening and Night variants. The native scene shows the selected
variant before it is used. These change appearance through the existing setup
planner; widget content, placement, sizes, applets and Islands enablement are kept.
Existing Moments and explicit widget overrides remain authoritative.

Choose the Weather city or an independent city, then **Enable daily changes**.
The action adds three ordinary rules to Schedules: sunrise, evening golden hour
and sunset. It does not replay past switches immediately. Rules remain editable
there and obey existing pause, priority and manual-change protection. A duplicate
cycle is rejected. Locations and zones are local preferences, excluded from
shared setups.

The three looks use coordinated colors and bounded gradient backgrounds with
no downloads. Native light/dark, compact/wide scenes were inspected. Tests cover
actual appearance application, repeat application, city availability, duplicate
activation and read-only previews. Final 0.5 package acceptance is separate.
