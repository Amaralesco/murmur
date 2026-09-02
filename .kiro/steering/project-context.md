# Project context

## What this is

`murmur` is a multi-tenant event ingestion and analytics pipeline built on the
Bluesky firehose. Users define a "project" as a filter over the firehose
(keywords, accounts, record types) and get real-time analytics on that slice.

## Why it exists

Deliberate skill development toward an ingestion pipeline engineer role
(PostHog-style: billions of events/month, Rust + Node, Kafka, ClickHouse,
PostgreSQL, Redis). The product has real utility, but the primary output is
demonstrable capability: benchmarks, profiling artifacts, incident postmortems,
and architecture decisions I can defend out loud.

See `docs/ROADMAP.md` for the plan and current position.
See `docs/DECISIONS.md` for why the stack looks like this.

## Non-negotiables

These are the things that make the project count. Do not let me skip them.

1. **Instrumented from the first commit.** Throughput, consumer lag, and p99
   latency exist before any optimisation. Never optimise without a measurement.
2. **Runs 24/7 against the live firehose.** Not a demo I start by hand. The
   operational pain is the point.
3. **Every number is reproducible** against the recorded replay corpus.
4. **Real failures, written up.** Each injected or organic failure gets a
   postmortem in `docs/postmortems/`.
5. **I must be able to explain every line.** If I cannot defend a design
   decision without assistance, it does not ship.

## What "done" looks like for a phase

A working end-to-end path, a number that moved, and a paragraph explaining why.
Not a complete feature set. Ship the core, then layer.
