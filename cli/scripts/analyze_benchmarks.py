#!/usr/bin/env python3
"""
Benchmark Analysis Script for AI Passport Provers

Analyzes benchmark JSONL files and produces statistical summaries suitable
for scientific papers, including:
- Mean, standard deviation, and 95% confidence intervals
- Per-round analysis
- Statistical significance tests between prover types
- Recommendations for sample size adequacy

Usage:
    python analyze_benchmarks.py <file.jsonl> [file2.jsonl ...]
    python analyze_benchmarks.py benchmarks/*.jsonl
    python analyze_benchmarks.py benchmarks/anthropic_*.jsonl --format csv
"""

import argparse
import json
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

import numpy as np
from scipy import stats


@dataclass
class RoundStats:
    """Statistics for a single round across multiple runs."""
    round_num: int
    durations_ms: list[float] = field(default_factory=list)
    request_bytes: list[int] = field(default_factory=list)
    response_bytes: list[int] = field(default_factory=list)


@dataclass
class ProverStats:
    """Aggregated statistics for a prover type."""
    prover_type: str
    notary_domain: Optional[str]
    total_durations_ms: list[float] = field(default_factory=list)
    setup_times_ms: list[float] = field(default_factory=list)
    rounds_stats: dict[int, RoundStats] = field(default_factory=dict)
    all_round_durations_ms: list[float] = field(default_factory=list)
    success_count: int = 0
    failure_count: int = 0


def compute_ci(data: list[float], confidence: float = 0.95) -> tuple[float, float, float]:
    """
    Compute mean and confidence interval.
    Returns (mean, ci_lower, ci_upper).
    """
    if len(data) < 2:
        mean = np.mean(data) if data else 0.0
        return mean, mean, mean

    mean = np.mean(data)
    sem = stats.sem(data)
    ci = stats.t.interval(confidence, len(data) - 1, loc=mean, scale=sem)
    return mean, ci[0], ci[1]


def compute_stats(data: list[float]) -> dict:
    """Compute comprehensive statistics for a dataset."""
    if not data:
        return {
            "n": 0, "mean": 0, "std": 0, "sem": 0,
            "ci_lower": 0, "ci_upper": 0,
            "min": 0, "max": 0, "median": 0,
            "cv": 0, "adequate_sample": False
        }

    n = len(data)
    mean = np.mean(data)
    std = np.std(data, ddof=1) if n > 1 else 0
    sem = stats.sem(data) if n > 1 else 0

    mean, ci_lower, ci_upper = compute_ci(data)

    # Coefficient of variation (CV) - measure of relative variability
    cv = (std / mean * 100) if mean > 0 else 0

    # Rule of thumb: CV < 15% and n >= 5 is often adequate for scientific reporting
    # Also check if CI width is < 20% of mean
    ci_width_pct = ((ci_upper - ci_lower) / mean * 100) if mean > 0 else 100
    adequate_sample = n >= 5 and cv < 30 and ci_width_pct < 40

    return {
        "n": n,
        "mean": mean,
        "std": std,
        "sem": sem,
        "ci_lower": ci_lower,
        "ci_upper": ci_upper,
        "min": np.min(data),
        "max": np.max(data),
        "median": np.median(data),
        "cv": cv,
        "ci_width_pct": ci_width_pct,
        "adequate_sample": adequate_sample
    }


def parse_prover_type(prover: dict) -> tuple[str, Optional[str], Optional[str]]:
    """Extract prover type, notary/proxy domain, and network optimization from prover config."""
    if "Direct" in prover:
        return "Direct", None, None
    elif "Proxy" in prover:
        # ProxyProver has nested structure: {"Proxy": {"proxy": {"host": ..., "port": ...}}}
        proxy_wrapper = prover["Proxy"]
        proxy_config = proxy_wrapper.get("proxy", proxy_wrapper)  # Handle both nested and flat
        host = proxy_config.get("host", "")
        # Distinguish TEE vs non-TEE proxy based on host
        if "tee" in host.lower():
            return "Proxy-TEE", host, None
        else:
            return "Proxy", host, None
    elif "TlsSingleShot" in prover:
        notary = prover["TlsSingleShot"]["notary"]
        return "TlsSingleShot", notary.get("domain"), notary.get("network_optimization")
    elif "TlsPerMessage" in prover:
        notary = prover["TlsPerMessage"]["notary"]
        return "TlsPerMessage", notary.get("domain"), notary.get("network_optimization")
    else:
        return str(list(prover.keys())[0]), None, None


def load_and_aggregate(files: list[Path]) -> dict[str, ProverStats]:
    """Load benchmark files and aggregate by prover type and network optimization."""
    provers: dict[str, ProverStats] = {}

    for filepath in files:
        with open(filepath) as f:
            for line in f:
                if not line.strip():
                    continue
                record = json.loads(line)

                prover_type, notary_domain, network_opt = parse_prover_type(record["prover"])
                key = f"{prover_type}"
                if notary_domain:
                    # Shorten domain for display
                    short_domain = notary_domain.split(".")[0] if "." in notary_domain else notary_domain
                    key = f"{prover_type} ({short_domain})"
                if network_opt:
                    # Add network optimization to key (Bandwidth/Latency)
                    key = f"{key} [{network_opt}]"

                if key not in provers:
                    provers[key] = ProverStats(
                        prover_type=prover_type,
                        notary_domain=notary_domain
                    )

                ps = provers[key]

                if record.get("success", False):
                    ps.success_count += 1
                    results = record["results"]

                    ps.total_durations_ms.append(results["total_duration_ms"])
                    if results.get("setup_time_ms"):
                        ps.setup_times_ms.append(results["setup_time_ms"])

                    for round_data in results.get("rounds", []):
                        round_num = round_data["round"]
                        if round_num not in ps.rounds_stats:
                            ps.rounds_stats[round_num] = RoundStats(round_num=round_num)

                        rs = ps.rounds_stats[round_num]
                        rs.durations_ms.append(round_data["duration_ms"])
                        rs.request_bytes.append(round_data["request_bytes"])
                        rs.response_bytes.append(round_data["response_bytes"])

                        ps.all_round_durations_ms.append(round_data["duration_ms"])
                else:
                    ps.failure_count += 1

    return provers


def perform_significance_tests(provers: dict[str, ProverStats]) -> list[dict]:
    """Perform pairwise t-tests between prover types."""
    results = []
    prover_keys = list(provers.keys())

    for i, key1 in enumerate(prover_keys):
        for key2 in prover_keys[i+1:]:
            ps1 = provers[key1]
            ps2 = provers[key2]

            if len(ps1.total_durations_ms) >= 2 and len(ps2.total_durations_ms) >= 2:
                # Welch's t-test (doesn't assume equal variances)
                t_stat, p_value = stats.ttest_ind(
                    ps1.total_durations_ms,
                    ps2.total_durations_ms,
                    equal_var=False
                )

                # Effect size (Cohen's d)
                pooled_std = np.sqrt(
                    (np.var(ps1.total_durations_ms, ddof=1) + np.var(ps2.total_durations_ms, ddof=1)) / 2
                )
                cohens_d = (np.mean(ps1.total_durations_ms) - np.mean(ps2.total_durations_ms)) / pooled_std if pooled_std > 0 else 0

                results.append({
                    "comparison": f"{key1} vs {key2}",
                    "t_statistic": t_stat,
                    "p_value": p_value,
                    "cohens_d": cohens_d,
                    "significant_005": p_value < 0.05,
                    "significant_001": p_value < 0.01
                })

    return results


def format_time(ms: float) -> str:
    """Format milliseconds as human-readable time."""
    if ms >= 1000:
        return f"{ms/1000:.2f}s"
    return f"{ms:.0f}ms"


def format_ci(stats_dict: dict, unit: str = "ms") -> str:
    """Format confidence interval as string."""
    mean = stats_dict["mean"]
    ci_lower = stats_dict["ci_lower"]
    ci_upper = stats_dict["ci_upper"]

    if unit == "s":
        return f"{mean/1000:.2f}s [{ci_lower/1000:.2f}, {ci_upper/1000:.2f}]"
    return f"{mean:.0f}ms [{ci_lower:.0f}, {ci_upper:.0f}]"


def print_report(provers: dict[str, ProverStats], significance_tests: list[dict]):
    """Print a comprehensive report."""
    print("=" * 80)
    print("BENCHMARK ANALYSIS REPORT")
    print("=" * 80)
    print()

    for key, ps in sorted(provers.items()):
        print(f"\n{'─' * 80}")
        print(f"PROVER: {key}")
        print(f"{'─' * 80}")

        # Success rate
        total_runs = ps.success_count + ps.failure_count
        success_rate = (ps.success_count / total_runs * 100) if total_runs > 0 else 0
        print(f"\nRuns: {ps.success_count} successful / {total_runs} total ({success_rate:.1f}% success)")

        if ps.success_count == 0:
            print("  No successful runs to analyze.")
            continue

        # Total duration
        total_stats = compute_stats(ps.total_durations_ms)
        print(f"\n📊 TOTAL DURATION")
        print(f"  Mean:     {format_time(total_stats['mean'])} ± {format_time(total_stats['std'])} (std)")
        print(f"  95% CI:   {format_ci(total_stats, 's')}")
        print(f"  Range:    [{format_time(total_stats['min'])}, {format_time(total_stats['max'])}]")
        print(f"  CV:       {total_stats['cv']:.1f}%")

        # Setup time
        if ps.setup_times_ms:
            setup_stats = compute_stats(ps.setup_times_ms)
            print(f"\n⏱️  SETUP TIME")
            print(f"  Mean:     {format_time(setup_stats['mean'])} ± {format_time(setup_stats['std'])} (std)")
            print(f"  95% CI:   {format_ci(setup_stats, 's')}")
            print(f"  CV:       {setup_stats['cv']:.1f}%")

        # Average round duration (across all rounds)
        if ps.all_round_durations_ms:
            avg_round_stats = compute_stats(ps.all_round_durations_ms)
            print(f"\n🔄 AVERAGE ROUND DURATION (across all rounds)")
            print(f"  Mean:     {format_time(avg_round_stats['mean'])} ± {format_time(avg_round_stats['std'])} (std)")
            print(f"  95% CI:   {format_ci(avg_round_stats)}")
            print(f"  N:        {avg_round_stats['n']} round observations")

        # Per-round breakdown
        if ps.rounds_stats:
            print(f"\n📈 PER-ROUND BREAKDOWN")
            for round_num in sorted(ps.rounds_stats.keys()):
                rs = ps.rounds_stats[round_num]
                round_stats = compute_stats(rs.durations_ms)
                print(f"  Round {round_num}: {format_time(round_stats['mean'])} ± {format_time(round_stats['std'])} "
                      f"(n={round_stats['n']}, CV={round_stats['cv']:.1f}%)")

        # Sample adequacy
        print(f"\n📏 SAMPLE ADEQUACY")
        if total_stats["adequate_sample"]:
            print(f"  ✅ Sample size appears adequate for scientific reporting")
            print(f"     (n={total_stats['n']}, CV={total_stats['cv']:.1f}%, CI width={total_stats['ci_width_pct']:.1f}% of mean)")
        else:
            reasons = []
            if total_stats["n"] < 5:
                reasons.append(f"n={total_stats['n']} < 5")
            if total_stats["cv"] >= 30:
                reasons.append(f"CV={total_stats['cv']:.1f}% >= 30%")
            if total_stats["ci_width_pct"] >= 40:
                reasons.append(f"CI width={total_stats['ci_width_pct']:.1f}% >= 40% of mean")

            print(f"  ⚠️  Consider more runs for reliable statistics")
            print(f"     Reasons: {', '.join(reasons)}")

            # Estimate needed sample size for 10% CI width
            if total_stats["std"] > 0 and total_stats["mean"] > 0:
                target_ci_width = total_stats["mean"] * 0.1  # 10% of mean
                z = 1.96  # 95% CI
                estimated_n = int(np.ceil((z * total_stats["std"] / (target_ci_width / 2)) ** 2))
                print(f"     Estimated n for 10% CI width: ~{max(estimated_n, 5)}")

    # Statistical significance tests
    if significance_tests:
        print(f"\n\n{'=' * 80}")
        print("STATISTICAL SIGNIFICANCE TESTS (Welch's t-test)")
        print("=" * 80)

        for test in significance_tests:
            print(f"\n{test['comparison']}:")
            print(f"  t-statistic: {test['t_statistic']:.3f}")
            print(f"  p-value:     {test['p_value']:.4f}", end="")
            if test["significant_001"]:
                print(" ***")
            elif test["significant_005"]:
                print(" **")
            else:
                print("")
            print(f"  Cohen's d:   {test['cohens_d']:.3f} ", end="")
            if abs(test['cohens_d']) < 0.2:
                print("(negligible)")
            elif abs(test['cohens_d']) < 0.5:
                print("(small)")
            elif abs(test['cohens_d']) < 0.8:
                print("(medium)")
            else:
                print("(large)")

            if test["significant_005"]:
                print(f"  → Difference is statistically significant (p < 0.05)")
            else:
                print(f"  → No statistically significant difference detected")

    print()


def export_csv(provers: dict[str, ProverStats], output_file: Optional[str] = None):
    """Export statistics to CSV for plotting."""
    import csv
    import sys

    output = open(output_file, 'w', newline='') if output_file else sys.stdout

    writer = csv.writer(output)
    writer.writerow([
        "prover_type", "metric", "n", "mean", "std", "ci_lower", "ci_upper",
        "min", "max", "cv", "adequate"
    ])

    for key, ps in sorted(provers.items()):
        if ps.success_count == 0:
            continue

        # Total duration
        s = compute_stats(ps.total_durations_ms)
        writer.writerow([
            key, "total_duration_ms", s["n"], f"{s['mean']:.2f}", f"{s['std']:.2f}",
            f"{s['ci_lower']:.2f}", f"{s['ci_upper']:.2f}",
            f"{s['min']:.2f}", f"{s['max']:.2f}", f"{s['cv']:.2f}", s["adequate_sample"]
        ])

        # Setup time
        if ps.setup_times_ms:
            s = compute_stats(ps.setup_times_ms)
            writer.writerow([
                key, "setup_time_ms", s["n"], f"{s['mean']:.2f}", f"{s['std']:.2f}",
                f"{s['ci_lower']:.2f}", f"{s['ci_upper']:.2f}",
                f"{s['min']:.2f}", f"{s['max']:.2f}", f"{s['cv']:.2f}", s["adequate_sample"]
            ])

        # Average round
        if ps.all_round_durations_ms:
            s = compute_stats(ps.all_round_durations_ms)
            writer.writerow([
                key, "avg_round_duration_ms", s["n"], f"{s['mean']:.2f}", f"{s['std']:.2f}",
                f"{s['ci_lower']:.2f}", f"{s['ci_upper']:.2f}",
                f"{s['min']:.2f}", f"{s['max']:.2f}", f"{s['cv']:.2f}", s["adequate_sample"]
            ])

        # Per-round
        for round_num in sorted(ps.rounds_stats.keys()):
            rs = ps.rounds_stats[round_num]
            s = compute_stats(rs.durations_ms)
            writer.writerow([
                key, f"round_{round_num}_duration_ms", s["n"], f"{s['mean']:.2f}", f"{s['std']:.2f}",
                f"{s['ci_lower']:.2f}", f"{s['ci_upper']:.2f}",
                f"{s['min']:.2f}", f"{s['max']:.2f}", f"{s['cv']:.2f}", s["adequate_sample"]
            ])

    if output_file:
        output.close()
        print(f"CSV exported to: {output_file}")


def export_json(provers: dict[str, ProverStats], significance_tests: list[dict], output_file: Optional[str] = None):
    """Export statistics to JSON for programmatic use."""
    import sys

    data = {
        "provers": {},
        "significance_tests": significance_tests
    }

    for key, ps in sorted(provers.items()):
        if ps.success_count == 0:
            continue

        prover_data = {
            "success_count": ps.success_count,
            "failure_count": ps.failure_count,
            "total_duration": compute_stats(ps.total_durations_ms),
            "setup_time": compute_stats(ps.setup_times_ms) if ps.setup_times_ms else None,
            "avg_round_duration": compute_stats(ps.all_round_durations_ms) if ps.all_round_durations_ms else None,
            "per_round": {}
        }

        for round_num in sorted(ps.rounds_stats.keys()):
            rs = ps.rounds_stats[round_num]
            prover_data["per_round"][round_num] = {
                "duration": compute_stats(rs.durations_ms),
                "request_bytes": compute_stats([float(x) for x in rs.request_bytes]),
                "response_bytes": compute_stats([float(x) for x in rs.response_bytes])
            }

        data["provers"][key] = prover_data

    output = json.dumps(data, indent=2, default=float)

    if output_file:
        with open(output_file, 'w') as f:
            f.write(output)
        print(f"JSON exported to: {output_file}")
    else:
        print(output)


def main():
    parser = argparse.ArgumentParser(
        description="Analyze benchmark JSONL files and produce statistical summaries.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  %(prog)s benchmarks/anthropic_*.jsonl
  %(prog)s benchmarks/*.jsonl --format csv --output results.csv
  %(prog)s file1.jsonl file2.jsonl --format json
        """
    )
    parser.add_argument("files", nargs="+", type=Path, help="JSONL benchmark files to analyze")
    parser.add_argument("--format", "-f", choices=["report", "csv", "json"], default="report",
                        help="Output format (default: report)")
    parser.add_argument("--output", "-o", type=str, help="Output file (default: stdout for csv/json)")

    args = parser.parse_args()

    # Expand globs and validate files
    files = []
    for pattern in args.files:
        if pattern.exists():
            files.append(pattern)
        else:
            # Try glob
            matched = list(Path(".").glob(str(pattern)))
            if matched:
                files.extend(matched)
            else:
                print(f"Warning: No files matching '{pattern}'", file=sys.stderr)

    if not files:
        print("Error: No benchmark files found.", file=sys.stderr)
        sys.exit(1)

    print(f"Analyzing {len(files)} file(s)...", file=sys.stderr)

    # Load and aggregate data
    provers = load_and_aggregate(files)

    if not provers:
        print("Error: No benchmark data found in files.", file=sys.stderr)
        sys.exit(1)

    # Perform significance tests
    significance_tests = perform_significance_tests(provers)

    # Output
    if args.format == "report":
        print_report(provers, significance_tests)
    elif args.format == "csv":
        export_csv(provers, args.output)
    elif args.format == "json":
        export_json(provers, significance_tests, args.output)


if __name__ == "__main__":
    main()