import argparse
import http.client
import json
import random
import time
from typing import List, Dict, Tuple
from dataclasses import dataclass
from collections import defaultdict
import sys

import matplotlib.pyplot as plt
import numpy as np

@dataclass
class BenchmarkResult:
    """Store results from a single benchmark run"""
    experiment: str
    network_size: int
    target_size: int
    duration: float
    success: bool
    details: str = ""


class ChordNode:
    """Wrapper for interacting with a Chord node via HTTP API"""
    
    def __init__(self, address: str):
        self.address = address  # format: "host:port"
        self.host, port_str = address.split(':')
        self.port = int(port_str)
    
    def join(self, nprime_address: str, timeout: int = 10) -> bool:
        """Join the Chord network via nprime node"""
        conn = None
        try:
            conn = http.client.HTTPConnection(self.address, timeout=timeout)
            conn.request("POST", f"/join?nprime={nprime_address}")
            response = conn.getresponse()
            return response.status == 200
        except Exception as e:
            print(f"Error joining {self.address}: {e}", file=sys.stderr)
            return False
        finally:
            if conn:
                conn.close()
    
    def leave(self, timeout: int = 10) -> bool:
        """Leave the Chord network gracefully"""
        conn = None
        try:
            conn = http.client.HTTPConnection(self.address, timeout=timeout)
            conn.request("POST", "/leave")
            response = conn.getresponse()
            return response.status == 200
        except Exception as e:
            print(f"Error leaving {self.address}: {e}", file=sys.stderr)
            return False
        finally:
            if conn:
                conn.close()
    
    def reset(self, timeout: int = 10) -> bool:
        """Reset the node to initial single-node state (fast, doesn't notify others)"""
        conn = None
        try:
            conn = http.client.HTTPConnection(self.address, timeout=timeout)
            conn.request("POST", "/reset")
            response = conn.getresponse()
            return response.status == 200
        except Exception as e:
            print(f"Error resetting {self.address}: {e}", file=sys.stderr)
            return False
        finally:
            if conn:
                conn.close()
    
    def simulate_crash(self, timeout: int = 10) -> bool:
        """Simulate a crash on this node"""
        conn = None
        try:
            conn = http.client.HTTPConnection(self.address, timeout=timeout)
            conn.request("POST", "/sim-crash")
            response = conn.getresponse()
            return response.status == 200
        except Exception as e:
            print(f"Error crashing {self.address}: {e}", file=sys.stderr)
            return False
        finally:
            if conn:
                conn.close()
    
    def simulate_recover(self, timeout: int = 10) -> bool:
        """Recover from simulated crash"""
        conn = None
        try:
            conn = http.client.HTTPConnection(self.address, timeout=timeout)
            conn.request("POST", "/sim-recover")
            response = conn.getresponse()
            return response.status == 200
        except Exception as e:
            print(f"Error recovering {self.address}: {e}", file=sys.stderr)
            return False
        finally:
            if conn:
                conn.close()
    
    def get_node_info(self, timeout: int = 5) -> Dict:
        """Get node information including successor and known nodes"""
        conn = None
        try:
            conn = http.client.HTTPConnection(self.address, timeout=timeout)
            conn.request("GET", "/node-info")
            response = conn.getresponse()
            if response.status == 200:
                return json.loads(response.read().decode('utf-8'))
            return {}
        except Exception as e:
            print(f"Error getting node info from {self.address}: {e}", file=sys.stderr)
            return {}
        finally:
            if conn:
                conn.close()
    
    def ping(self, timeout: int = 5) -> bool:
        """Check if node is alive"""
        conn = None
        try:
            conn = http.client.HTTPConnection(self.address, timeout=timeout)
            conn.request("GET", "/helloworld")
            response = conn.getresponse()
            return response.status == 200
        except Exception:
            return False
        finally:
            if conn:
                conn.close()
    
    def put(self, key: str, value: str, timeout: int = 10) -> bool:
        """Store a key-value pair in the DHT"""
        conn = None
        try:
            conn = http.client.HTTPConnection(self.address, timeout=timeout)
            conn.request("PUT", f"/storage/{key}", body=value.encode('utf-8'))
            response = conn.getresponse()
            response.read()
            return response.status == 200
        except Exception as e:
            print(f"Error putting key {key} to {self.address}: {e}", file=sys.stderr)
            return False
        finally:
            if conn:
                conn.close()
    
    def get(self, key: str, timeout: int = 10) -> tuple[bool, str]:
        """Retrieve a value from the DHT"""
        conn = None
        try:
            conn = http.client.HTTPConnection(self.address, timeout=timeout)
            conn.request("GET", f"/storage/{key}")
            response = conn.getresponse()
            body = response.read().decode('utf-8')
            if response.status == 200:
                return True, body
            return False, ""
        except Exception as e:
            print(f"Error getting key {key} from {self.address}: {e}", file=sys.stderr)
            return False, ""
        finally:
            if conn:
                conn.close()


class NetworkStabilityChecker:
    """Utility class to check network stability and data integrity"""
    def is_stable(nodes: List[ChordNode], expected_size: int,  max_checks: int = 5, check_interval: float = 0.5) -> bool:
        consecutive_successes = 0
        
        for _ in range(max_checks * 2):  # Give more attempts
            if NetworkStabilityChecker._check_ring_consistency(nodes, expected_size):
                consecutive_successes += 1
                if consecutive_successes >= max_checks:
                    return True
            else:
                consecutive_successes = 0
            
            time.sleep(check_interval)
        
        return False
    
    def _check_ring_consistency(nodes: List[ChordNode], expected_size: int) -> bool:
        # Find a responsive node to start traversal
        start_node = None
        for node in nodes:
            if node.ping():
                start_node = node
                break
        
        if not start_node:
            return False
        
        # Traverse the ring by following successors
        visited = set()
        current_address = start_node.address
        start_address = current_address
        
        for _ in range(expected_size + 1):  # +1 to detect loops
            if current_address in visited and current_address == start_address:
                # We've completed the ring
                break
            
            visited.add(current_address)
            
            # Get successor of current node
            current_node = ChordNode(current_address)
            info = current_node.get_node_info()
            
            if not info or 'successor' not in info:
                return False
            
            successor = info['successor']
            
            # Check if successor is valid and different (unless single node)
            if not successor:
                return False
            
            # Move to successor
            current_address = successor
            
            # Prevent infinite loops
            if len(visited) > expected_size:
                return False
        
        # Check if we found exactly the expected number of nodes
        return len(visited) == expected_size and current_address == start_address
    
    def wait_for_stability(nodes: List[ChordNode], expected_size: int, timeout: float = 120.0) -> Tuple[bool, float]:
        start_time = time.time()
        
        while time.time() - start_time < timeout:
            if NetworkStabilityChecker.is_stable(nodes, expected_size):
                elapsed = time.time() - start_time
                return True, elapsed
            time.sleep(0.2)  # Reduced from 1.0s for faster detection
        
        return False, timeout


class ChordBenchmark:
    """Main benchmark orchestrator"""
    
    def __init__(self, node_addresses: List[str], repetitions: int = 3):
        self.all_nodes = [ChordNode(addr) for addr in node_addresses]
        self.repetitions = repetitions
        self.results: List[BenchmarkResult] = []
    
    def run_all_experiments(self):
        """Run all benchmark experiments"""
        print("=" * 80)
        print("CHORD NETWORK BENCHMARK")
        print("=" * 80)
        print(f"Available nodes: {len(self.all_nodes)}")
        print(f"Repetitions per experiment: {self.repetitions}")
        print()
        
        # Experiment 1: Network growth
        print("\n" + "=" * 80)
        print("EXPERIMENT 1: Network Growth Time")
        print("=" * 80)
        self.experiment_network_growth()
        
        # Experiment 2: Network shrinking
        print("\n" + "=" * 80)
        print("EXPERIMENT 2: Network Shrinking Time")
        print("=" * 80)
        self.experiment_network_shrinking()
        
        # Experiment 3: Crash tolerance
        print("\n" + "=" * 80)
        print("EXPERIMENT 3: Crash Tolerance")
        print("=" * 80)
        self.experiment_crash_tolerance()
        
        # Generate plots
        print("\n" + "=" * 80)
        print("GENERATING PLOTS")
        print("=" * 80)
        self.plot_results()
    
    def experiment_network_growth(self):
        network_sizes = [2, 4, 8, 16, 32]
        
        for size in network_sizes:
            if size > len(self.all_nodes):
                print(f"Skipping size {size}: not enough nodes available")
                continue
            
            print(f"\nTesting network growth to {size} nodes...")
            
            for rep in range(self.repetitions):
                print(f"  Repetition {rep + 1}/{self.repetitions}...", end=" ", flush=True)
                
                # HARD RESET: Reset ALL available nodes to prevent leftover state
                for node in self.all_nodes:
                    node.reset()
                
                # Select nodes for this test
                nodes = random.sample(self.all_nodes, size)
                
                # Brief wait for async cleanup from reset
                time.sleep(0.5)
                
                # Verify nodes are alive
                alive_nodes = [n for n in nodes if n.ping()]
                if len(alive_nodes) != size:
                    print(f"FAILED (only {len(alive_nodes)}/{size} nodes alive)")
                    self.results.append(BenchmarkResult(
                        "growth", size, size, -1, False, "Not all nodes alive"
                    ))
                    continue
                
                # Issue join calls (burst)
                seed_node = nodes[0]
                for node in nodes[1:]:
                    node.join(seed_node.address)
                
                # Brief wait for join RPCs to complete
                time.sleep(0.5)
                
                # Wait for network to stabilize
                success, elapsed = NetworkStabilityChecker.wait_for_stability(
                    nodes, size, timeout=120.0
                )
                
                if success:
                    print(f"SUCCESS ({elapsed:.2f}s)")
                    self.results.append(BenchmarkResult(
                        "growth", size, size, elapsed, True
                    ))
                else:
                    print(f"TIMEOUT (>{elapsed:.2f}s)")
                    self.results.append(BenchmarkResult(
                        "growth", size, size, elapsed, False, "Timeout waiting for stability"
                    ))
    
    def experiment_network_shrinking(self):
        test_cases = [(32, 16), (16, 8), (8, 4), (4, 2)]
        
        for start_size, end_size in test_cases:
            if start_size > len(self.all_nodes):
                print(f"Skipping {start_size}→{end_size}: not enough nodes available")
                continue
            
            print(f"\nTesting network shrinking from {start_size} to {end_size} nodes...")
            
            for rep in range(self.repetitions):
                print(f"  Repetition {rep + 1}/{self.repetitions}...", end=" ", flush=True)
                
                # HARD RESET: Reset ALL available nodes to prevent leftover state
                for node in self.all_nodes:
                    node.reset()
                
                # Select nodes for this test
                all_test_nodes = random.sample(self.all_nodes, start_size)
                
                # Brief wait for async cleanup
                time.sleep(0.5)
                
                # Build network to start_size
                seed_node = all_test_nodes[0]
                for node in all_test_nodes[1:]:
                    node.join(seed_node.address)
                
                # Wait for initial network to stabilize
                success, _ = NetworkStabilityChecker.wait_for_stability(
                    all_test_nodes, start_size, timeout=60.0
                )
                
                if not success:
                    print("FAILED (initial network didn't stabilize)")
                    self.results.append(BenchmarkResult(
                        "shrink", start_size, end_size, -1, False, 
                        "Initial network didn't stabilize"
                    ))
                    continue
                
                # Decide which nodes will leave
                nodes_to_keep = all_test_nodes[:end_size]
                nodes_to_leave = all_test_nodes[end_size:]
                
                # Issue leave calls (burst approach)
                for node in nodes_to_leave:
                    node.leave()

                # Brief wait for leave RPCs to complete
                time.sleep(0.3)
                
                # Wait for network to stabilize at smaller size
                timeout = 120.0
                success, elapsed = NetworkStabilityChecker.wait_for_stability(
                    nodes_to_keep, end_size, timeout=timeout
                )
                
                if success:
                    print(f"SUCCESS ({elapsed:.2f}s)")
                    self.results.append(BenchmarkResult(
                        "shrink", start_size, end_size, elapsed, True
                    ))
                else:
                    print(f"TIMEOUT (>{elapsed:.2f}s)")
                    self.results.append(BenchmarkResult(
                        "shrink", start_size, end_size, elapsed, False,
                        "Timeout waiting for stability"
                    ))
    
    def experiment_crash_tolerance(self):
        network_size = 32
        max_crash_burst = min(16, network_size // 2)  # Test up to half the network
        
        if network_size > len(self.all_nodes):
            print(f"Skipping crash tolerance test: need {network_size} nodes, have {len(self.all_nodes)}")
            return
        
        crash_burst_sizes = list(range(1, max_crash_burst + 1))
        
        for burst_size in crash_burst_sizes:
            print(f"\nTesting crash tolerance with {burst_size} simultaneous crashes...")
            
            for rep in range(self.repetitions):
                print(f"  Repetition {rep + 1}/{self.repetitions}...", end=" ", flush=True)
                
                # HARD RESET: Reset ALL available nodes to prevent leftover state
                for node in self.all_nodes:
                    node.simulate_recover()
                    node.reset()
                
                # Select nodes for this test
                all_test_nodes = random.sample(self.all_nodes, network_size)
                
                # Brief wait for cleanup
                time.sleep(0.5)
                
                # Build initial network
                seed_node = all_test_nodes[0]
                for node in all_test_nodes[1:]:
                    node.join(seed_node.address)
                
                # Wait for initial network to stabilize
                success, _ = NetworkStabilityChecker.wait_for_stability(
                    all_test_nodes, network_size, timeout=60.0
                )
                
                if not success:
                    print("FAILED (initial network didn't stabilize)")
                    self.results.append(BenchmarkResult(
                        "crash", network_size, network_size - burst_size, -1, False,
                        "Initial network didn't stabilize"
                    ))
                    continue
                
                # Select nodes to crash (random selection)
                nodes_to_crash = random.sample(all_test_nodes, burst_size)
                surviving_nodes = [n for n in all_test_nodes if n not in nodes_to_crash]
                
                # Crash nodes simultaneously (burst)
                for node in nodes_to_crash:
                    node.simulate_crash()
                
                # Wait for network to stabilize with remaining nodes
                expected_size = network_size - burst_size
                success, elapsed = NetworkStabilityChecker.wait_for_stability(
                    surviving_nodes, expected_size, timeout=120.0
                )
                
                if success:
                    print(f"SUCCESS ({elapsed:.2f}s)")
                    self.results.append(BenchmarkResult(
                        "crash", network_size, expected_size, elapsed, True
                    ))
                else:
                    print(f"FAILED/TIMEOUT (>{elapsed:.2f}s)")
                    self.results.append(BenchmarkResult(
                        "crash", network_size, expected_size, elapsed, False,
                        f"Network couldn't tolerate {burst_size} crashes"
                    ))
                
                # Recover crashed nodes for next test
                for node in nodes_to_crash:
                    node.simulate_recover()
    
    def plot_results(self):
        """Generate plots for all experiments with error bars"""
        
        # Separate results by experiment
        growth_results = [r for r in self.results if r.experiment == "growth" and r.success]
        shrink_results = [r for r in self.results if r.experiment == "shrink" and r.success]
        crash_results = [r for r in self.results if r.experiment == "crash"]
        
        # Plot 1: Network Growth Time
        if growth_results:
            self._plot_growth(growth_results)
        
        # Plot 2: Network Shrinking Time
        if shrink_results:
            self._plot_shrinking(shrink_results)
        
        # Plot 3: Crash Tolerance
        if crash_results:
            self._plot_crash_tolerance(crash_results)
    
    def _plot_growth(self, results: List[BenchmarkResult]):
        """Plot network growth time vs network size"""
        # Group by network size and calculate statistics
        size_to_durations = defaultdict(list)
        for r in results:
            size_to_durations[r.network_size].append(r.duration)
        
        sizes = sorted(size_to_durations.keys())
        means = [np.mean(size_to_durations[s]) for s in sizes]
        stds = [np.std(size_to_durations[s]) for s in sizes]
        
        plt.figure(figsize=(10, 6))
        plt.errorbar(sizes, means, yerr=stds, marker='o', capsize=5, 
                    linewidth=2, markersize=8)
        plt.xlabel('Network Size (nodes)', fontsize=12)
        plt.ylabel('Time to Stabilize (seconds)', fontsize=12)
        plt.title('Network Growth Time vs Network Size', fontsize=14, fontweight='bold')
        plt.grid(True, alpha=0.3)
        plt.tight_layout()
        plt.savefig('network_growth.pdf')
        plt.savefig('network_growth.png', dpi=300)
        print("Saved: network_growth.pdf and network_growth.png")
        plt.close()
    
    def _plot_shrinking(self, results: List[BenchmarkResult]):
        """Plot network shrinking time"""
        # Group by (start_size, end_size) and calculate statistics
        transition_to_durations = defaultdict(list)
        for r in results:
            key = (r.network_size, r.target_size)
            transition_to_durations[key].append(r.duration)
        
        transitions = sorted(transition_to_durations.keys())
        labels = [f"{start}→{end}" for start, end in transitions]
        means = [np.mean(transition_to_durations[t]) for t in transitions]
        stds = [np.std(transition_to_durations[t]) for t in transitions]
        
        plt.figure(figsize=(10, 6))
        x_pos = np.arange(len(labels))
        plt.bar(x_pos, means, yerr=stds, capsize=5, alpha=0.7, color='coral', edgecolor='black')
        plt.xlabel('Network Transition', fontsize=12)
        plt.ylabel('Time to Stabilize (seconds)', fontsize=12)
        plt.title('Network Shrinking Time', fontsize=14, fontweight='bold')
        plt.xticks(x_pos, labels)
        plt.grid(True, alpha=0.3, axis='y')
        plt.tight_layout()
        plt.savefig('network_shrinking.pdf')
        plt.savefig('network_shrinking.png', dpi=300)
        print("Saved: network_shrinking.pdf and network_shrinking.png")
        plt.close()
    
    def _plot_crash_tolerance(self, results: List[BenchmarkResult]):
        """Plot crash tolerance results"""
        # Group by crash burst size
        burst_to_durations = defaultdict(list)
        burst_to_successes = defaultdict(list)
        
        for r in results:
            burst_size = r.network_size - r.target_size
            burst_to_durations[burst_size].append(r.duration if r.success else None)
            burst_to_successes[burst_size].append(r.success)
        
        burst_sizes = sorted(burst_to_durations.keys())
        
        # Plot 3a: Recovery time for successful cases
        successful_bursts = []
        successful_means = []
        successful_stds = []
        
        for burst in burst_sizes:
            durations = [d for d in burst_to_durations[burst] if d is not None]
            if durations:
                successful_bursts.append(burst)
                successful_means.append(np.mean(durations))
                successful_stds.append(np.std(durations))
        
        if successful_bursts:
            plt.figure(figsize=(10, 6))
            plt.errorbar(successful_bursts, successful_means, yerr=successful_stds, marker='o', capsize=5, linewidth=2, markersize=8, color='green')
            plt.xlabel('Crash Burst Size (number of simultaneous crashes)', fontsize=12)
            plt.ylabel('Recovery Time (seconds)', fontsize=12)
            plt.title('Network Recovery Time vs Crash Burst Size', fontsize=14, fontweight='bold')
            plt.grid(True, alpha=0.3)
            plt.tight_layout()
            plt.savefig('crash_recovery_time.pdf')
            plt.savefig('crash_recovery_time.png', dpi=300)
            print("Saved: crash_recovery_time.pdf and crash_recovery_time.png")
            plt.close()
        
        # Plot 3b: Success rate
        success_rates = []
        for burst in burst_sizes:
            successes = burst_to_successes[burst]
            success_rate = sum(successes) / len(successes) * 100
            success_rates.append(success_rate)
        
        plt.figure(figsize=(10, 6))
        colors = ['green' if rate == 100 else 'orange' if rate >= 50 else 'red' for rate in success_rates]
        plt.bar(burst_sizes, success_rates, color=colors, alpha=0.7, edgecolor='black')
        plt.xlabel('Crash Burst Size (number of simultaneous crashes)', fontsize=12)
        plt.ylabel('Success Rate (%)', fontsize=12)
        plt.title('Network Crash Tolerance: Success Rate', fontsize=14, fontweight='bold')
        plt.axhline(y=100, color='green', linestyle='--', alpha=0.5, label='100% success')
        plt.axhline(y=50, color='orange', linestyle='--', alpha=0.5, label='50% success')
        plt.ylim(0, 105)
        plt.grid(True, alpha=0.3, axis='y')
        plt.legend()
        plt.tight_layout()
        plt.savefig('crash_tolerance_rate.pdf')
        plt.savefig('crash_tolerance_rate.png', dpi=300)
        print("Saved: crash_tolerance_rate.pdf and crash_tolerance_rate.png")
        plt.close()
    
    def print_summary(self):
        """Print summary statistics"""
        print("\n" + "=" * 80)
        print("BENCHMARK SUMMARY")
        print("=" * 80)
        
        # Group results by experiment
        for exp_type in ["growth", "shrink", "crash"]:
            exp_results = [r for r in self.results if r.experiment == exp_type]
            if not exp_results:
                continue
            
            print(f"\n{exp_type.upper()} Experiment:")
            print(f"  Total runs: {len(exp_results)}")
            successful = [r for r in exp_results if r.success]
            print(f"  Successful: {len(successful)} ({len(successful)/len(exp_results)*100:.1f}%)")
            
            if successful:
                durations = [r.duration for r in successful]
                print(f"  Mean duration: {np.mean(durations):.2f}s")
                print(f"  Std deviation: {np.std(durations):.2f}s")
                print(f"  Min duration: {np.min(durations):.2f}s")
                print(f"  Max duration: {np.max(durations):.2f}s")


def main():
    parser = argparse.ArgumentParser()
    
    parser.add_argument(
        "nodes",
        type=str,
        nargs="+",
        help="Addresses (host:port) of available nodes to test"
    )
    
    parser.add_argument(
        "--repetitions",
        type=int,
        default=3,
        help="Number of repetitions per test (default: 3)"
    )
    
    args = parser.parse_args()
    
    # Remove duplicates
    node_addresses = list(set(args.nodes))
    
    if len(node_addresses) < 32:
        print(f"Warning: Only {len(node_addresses)} nodes available. Some tests may be skipped.")
        print("Recommend at least 32 nodes for complete benchmarking.")
        response = input("Continue anyway? [y/N]: ")
        if response.lower() != 'y':
            print("Aborted.")
            return
    
    # Create and run benchmark
    benchmark = ChordBenchmark(node_addresses, repetitions=args.repetitions)
    benchmark.run_all_experiments()
    benchmark.print_summary()
    
    print("\n" + "=" * 80)
    print("BENCHMARK COMPLETE")
    print("=" * 80)
    print("Results saved to:")
    print("  - network_growth.pdf / .png")
    print("  - network_shrinking.pdf / .png")
    print("  - crash_recovery_time.pdf / .png")
    print("  - crash_tolerance_rate.pdf / .png")


if __name__ == "__main__":
    main()
