#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("analyze_runtime_memory_log.py")
SPEC = importlib.util.spec_from_file_location("runtime_memory_analyzer", MODULE_PATH)
assert SPEC and SPEC.loader
analyzer = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = analyzer
SPEC.loader.exec_module(analyzer)

MB = 1024 * 1024


def sample(
    *,
    timestamp_ms: int,
    instance_id: str,
    pss_mb: int,
    allocated_mb: int,
    live_sessions: int | None = None,
    connected_clients: int = 0,
    total_json_mb: int = 0,
) -> analyzer.Sample:
    sessions = None
    kind = "process"
    if live_sessions is not None:
        kind = "attribution"
        sessions = {
            "live_count": live_sessions,
            "total_json_bytes": total_json_mb * MB,
            "total_payload_text_bytes": 0,
            "total_provider_cache_json_bytes": 0,
            "total_tool_result_bytes": 0,
            "total_large_blob_bytes": 0,
            "top_by_json_bytes": [],
        }
    raw = {
        "server": {"id": instance_id},
        "process": {
            "rss_bytes": pss_mb * MB,
            "os": {"pss_bytes": pss_mb * MB, "pss_anon_bytes": pss_mb * MB},
            "allocator": {
                "stats": {
                    "allocated_bytes": allocated_mb * MB,
                    "retained_bytes": 0,
                }
            },
        },
        "process_diagnostics": {"allocator_retained_resident_estimate_bytes": 0},
        "clients": {"connected_count": connected_clients},
        "sessions": sessions,
    }
    return analyzer.Sample(
        path=Path("server-runtime-memory-test.jsonl"),
        line_no=timestamp_ms,
        raw=raw,
        timestamp_ms=timestamp_ms,
        kind=kind,
        target="server",
        instance_id=instance_id,
        source=f"{kind}:test",
        trigger_category="test",
        trigger_reason="unit",
        sessions=sessions,
        totals=None,
    )


class RuntimeMemoryAnalyzerTests(unittest.TestCase):
    def test_latest_instance_filter_prevents_cross_reload_spikes(self) -> None:
        samples = [
            sample(timestamp_ms=1, instance_id="old", pss_mb=1800, allocated_mb=1700),
            sample(timestamp_ms=2, instance_id="old", pss_mb=1900, allocated_mb=1800),
            sample(timestamp_ms=3, instance_id="new", pss_mb=80, allocated_mb=40),
            sample(timestamp_ms=4, instance_id="new", pss_mb=120, allocated_mb=70),
        ]

        selected = analyzer.select_latest_instances(samples)

        self.assertEqual({item.instance_id for item in selected}, {"new"})
        summary = analyzer.process_summary(selected)
        self.assertEqual(summary["baseline_pss_bytes"], 80 * MB)
        self.assertEqual(summary["net_pss_growth_bytes"], 40 * MB)

    def test_explicit_instance_selection_preserves_historical_incident(self) -> None:
        samples = [
            sample(timestamp_ms=1, instance_id="old", pss_mb=70, allocated_mb=38, live_sessions=9),
            sample(
                timestamp_ms=2,
                instance_id="old",
                pss_mb=3900,
                allocated_mb=3700,
                live_sessions=1140,
                connected_clients=5,
            ),
            sample(timestamp_ms=3, instance_id="new", pss_mb=80, allocated_mb=40, live_sessions=2),
        ]

        selected = analyzer.select_instance(samples, "old")
        summary = analyzer.summarize_target(selected, top_n=5, min_spike_bytes=8 * MB)
        inventory = analyzer.instance_inventory(samples)

        self.assertEqual({item.instance_id for item in selected}, {"old"})
        self.assertEqual(summary["session_population"]["peak_live_sessions"], 1140)
        self.assertEqual(summary["incident"]["primary_cause"], "runaway_live_session_population")
        self.assertEqual([item["instance_id"] for item in inventory], ["new", "old"])

    def test_runaway_session_population_is_primary_cause(self) -> None:
        samples = [
            sample(
                timestamp_ms=1,
                instance_id="server",
                pss_mb=70,
                allocated_mb=38,
                live_sessions=9,
                connected_clients=0,
                total_json_mb=1,
            ),
            sample(
                timestamp_ms=2,
                instance_id="server",
                pss_mb=3900,
                allocated_mb=3700,
                live_sessions=1140,
                connected_clients=5,
                total_json_mb=360,
            ),
        ]

        summary = analyzer.summarize_target(samples, top_n=5, min_spike_bytes=8 * MB)
        incident = summary["incident"]
        population = summary["session_population"]

        self.assertEqual(incident["severity"], "critical")
        self.assertEqual(incident["primary_cause"], "runaway_live_session_population")
        self.assertEqual(incident["confidence"], "high")
        self.assertEqual(population["net_live_session_growth"], 1131)
        self.assertGreater(population["allocator_growth_per_added_session_bytes"], 2 * MB)
        self.assertIn("Pause or cap", incident["recommended_actions"][0]["action"])

    def test_allocator_retention_has_purge_first_action(self) -> None:
        current = sample(
            timestamp_ms=1,
            instance_id="server",
            pss_mb=1500,
            allocated_mb=500,
            live_sessions=8,
            connected_clients=5,
            total_json_mb=100,
        )
        current.raw["process_diagnostics"]["allocator_retained_resident_estimate_bytes"] = 600 * MB
        coverage = analyzer.build_coverage_report(current)
        incident = analyzer.build_incident_assessment(
            [current], analyzer.process_summary([current]), coverage, analyzer.session_population_summary([current])
        )

        self.assertEqual(incident["primary_cause"], "allocator_retention")
        self.assertIn("purge", incident["recommended_actions"][0]["action"].lower())


if __name__ == "__main__":
    unittest.main()
