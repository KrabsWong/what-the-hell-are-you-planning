# Collaboration cost assessment

Assessed 2026-11-10. This is a bounded local usage audit and API-equivalent estimate, not a billing reconciliation. It accompanies the [project collaboration playbook](../AGENT_COLLABORATION_PLAYBOOK.md).

## Findings

The user's approximate USD 1,220 API-equivalent estimate is plausible in scale. The primary Codex conversation alone estimates to **USD 1,170.93 through its first delivery**, or **USD 1,183.02 through the fixed later snapshot**, using the current standard rates specified below. These are not invoices and not a verified total for every related session.

The local OpenCode records support billions of cache-read tokens. The selected implementation-session cohort records about **5.680 billion cache reads**, not billions of newly generated output tokens. The user's roughly six-billion dashboard figure is directionally consistent but has not been reconciled to the dashboard's date filters, session population, normalization or fork accounting.

The user reports approximately USD 30 in the OpenCode Go subscription context. Its exact meaning (subscription payment, dashboard valuation, allocated usage or additional charge) has not been verified. Do not label it the measured marginal cash cost of this task.

## Scope and provenance

Read only:

- Codex primary rollout `01a1ba4b-c887-72d3-8cb4-c2bc576cde41`, token-count events and first-delivery timestamp.
- OpenCode local SQLite `session_v2` summary rows, opened with `mode=ro`.
- Project [post-delivery correction review](POST_DELIVERY_REVIEW.md).
- Official [API pricing](https://developers.openai.com/api/docs/pricing) and [prompt caching documentation](https://developers.openai.com/api/docs/guides/prompt-caching), accessed 2026-11-10.

No app databases were modified, no agent was launched, and no paid experiment was run for this assessment. Raw conversations, credentials and private billing data are not copied into this repository.

The primary conversation includes the initial audit, execution supervision, acceptance and later discussion. First delivery was recorded at **2026-11-09 10:43:07 UTC** (18:43:07 Asia/Shanghai). The later fixed snapshot is **2026-11-09 16:32:24 UTC** (2026-11-10 00:32:24 Asia/Shanghai), during this assessment. It includes post-delivery work and must not be called the exact 23-hour goal total.

The approximately 23-hour duration is user-reported. Exact goal start/end and active-versus-waiting time have not been reconstructed. The current local goal table has no row for the primary conversation; goal continuation messages are not sufficient to establish a complete billing boundary.

## Codex counters and estimate

Observed turn contexts name `gpt-6-astra`. Use the last cumulative token-count snapshot at or before each cutoff; **do not sum cumulative events**. The inspected primary rollout had no decrease in cumulative total across 7,236 observed token-count events. Duplicate unchanged snapshots therefore do not inflate the selected endpoint. This check does not establish independent accounting for forks or other sessions.

| Counter | Through first delivery | Later fixed snapshot |
| --- | ---: | ---: |
| Input, including cache reads | 1,006,882,790 | 1,016,589,609 |
| Cached input | 997,498,880 | 1,007,060,608 |
| Uncached input: input minus cached input | 9,383,910 | 9,529,001 |
| Output | 1,591,902 | 1,613,313 |
| Reasoning output, reported separately | 325,067 | 327,080 |
| Cache writes | 0 | 0 |
| Total: input plus output | 1,008,474,692 | 1,018,202,922 |
| Cached share of input | 99.068% | 99.063% |

The observed usage metadata says reasoning is included; do not add reasoning output again to the output charge. Cached input is a subset of input here; do not add it again to total tokens.

Pricing assumption: current GPT-6 Astra standard short-context rates, USD **10 / 1 million uncached input**, **1 / 1 million cached input**, **50 / 1 million output**. The official pricing page distinguishes processing tiers and context categories. This calculation assumes standard pricing throughout; actual historical tier, context surcharges, tools and account-specific arrangements have not been reconciled. It is a current-price comparison, not a reconstruction of an API invoice.

```text
API equivalent =
  uncached_input / 1,000,000 * 10
  + cached_input / 1,000,000 * 1
  + output / 1,000,000 * 50

First delivery:
  93.83910 + 997.49888 + 79.59510 = USD 1,170.93308

Later fixed snapshot:
  95.29001 + 1,007.060608 + 80.66565 = USD 1,183.016268
```

Cache reads account for approximately 85.2% of the first-delivery estimate despite their discounted unit rate. High cache hit rate is therefore not evidence of low total cost. Repeatedly supplying long context can dominate the equivalent bill; the counters alone do not prove which calls were unnecessary.

As a rough boundary sensitivity check, subtracting the cumulative snapshot immediately before the first observed goal continuation (2026-11-08 09:51:50.605 UTC; USD 50.86 equivalent) gives about **USD 1,120.07** through first delivery. This is only a proxy window: it is not the verified goal-start baseline and must not replace the broader, clearly defined figure above.

Excluded/unresolved: independently consumed tokens in related chats, internal/subagent/automatic-review sessions, inherited history in forked chats, any unlogged provider usage, tools, and subsequent user corrections. Related thread summary counters must not simply be added: inherited cumulative history may overlap. Request-level or fork-baseline reconciliation is required for an all-session total.

## OpenCode local cohort

Selection: `session_v2.directory` equals this repository's root, and `time_created >= 1794126891000` (the primary Codex conversation creation time). This yields **32 session rows**, including an empty setup row. Nonempty selected rows name `deepseek-v4.1-flash` via `opencode-go`. Latest selected session update is 2026-11-09 10:41:47.621 UTC, before first delivery.

| Raw summary field | Sum |
| --- | ---: |
| `tokens_input` | 25,183,709 |
| `tokens_cache_read` | 5,679,537,898 |
| `tokens_cache_write` | 0 |
| `tokens_output` | 4,416,381 |
| `tokens_reasoning` | 4,137,227 |
| `cost` | 0.093180655 |

These are raw local summary fields, not normalized billable usage. Output/reasoning inclusion semantics and inherited messages in copied/forked sessions were not reconciled request by request. Do not sum all fields into a definitive billable token total or interpret the stored `cost` as the Go subscription invoice. The wider repository history also includes work preceding this optimization, which can explain part of the difference from a dashboard total.

Reproduction query, against a read-only SQLite connection:

```sql
SELECT count(*) AS sessions,
       sum(tokens_input), sum(tokens_output), sum(tokens_reasoning),
       sum(tokens_cache_read), sum(tokens_cache_write), sum(cost)
FROM session_v2
WHERE directory = :repository_root
  AND time_created >= 1794126891000;
```

To close the remaining accounting gap, export dashboard usage for the exact time range with model, request/session IDs, cache categories and fee definitions; map the implementation sessions and forks; then reconcile unique requests against local counters. No additional access or dashboard interaction was required for this preliminary assessment.

## Evaluate economics correctly

Keep separate measures:

1. **Incremental cash:** additional purchases, overages and attributable infrastructure actually paid because of this task. Existing flat subscription fees are not automatically incremental charges.
2. **Allocated subscription cost:** a documented, consistent allocation of paid subscription fees across projects. It is a management estimate, not provider billing.
3. **API-equivalent cost:** normalized usage at a dated, named price/tier. Useful for portability and subscription-dependence analysis; it is neither cash paid nor provider production cost.
4. **Human and delivery cost:** user supervision, manual correction time, elapsed delay and escaped defects. Do not count 23 elapsed hours as 23 hours of human labor or model execution.

```text
Cash-based delivery cost = attributable cash + valued human time
Allocated delivery cost = allocated subscription/infra + valued human time
API scenario cost = API-equivalent usage + tool/infra costs + valued human time
```

These are alternative views; do not add all three monetary measures together. If subscriptions remain unchanged, marginal cash may be small while quota consumption, delay and correction burden remain substantial.

This run cannot yet establish savings versus strong-model-only execution: there was no controlled comparable baseline, and actual human correction minutes were not recorded. The post-delivery review demonstrates specification and acceptance failures, so blaming execution-model quality alone is unsupported.

For the next batch, compare similar-risk tasks under direct execution and delegation. Record accepted-outcome cost, first-pass acceptance, rework share, elapsed time and user correction minutes. The first improvement target should be unnecessary long-context coordination and incorrect task/acceptance definitions, alongside model routing—not merely a cheaper executor.
