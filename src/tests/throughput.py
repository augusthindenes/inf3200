# We want to test the throughput of our Chord DHT with 32 nodes.
# Each test will be run 3 times to get an average (with a standard deviation).
# Each test will consist of putting and getting 1000 key-value pairs.

import http.client
import json
import random
import string
import time
import uuid
import matplotlib.pyplot as plt
import argparse
import sys

def arg_parser():
    parser = argparse.ArgumentParser(prog="client", description="DHT client")

    parser.add_argument("nodes", type=str, nargs="+",
            help="addresses (host:port) of nodes to test")

    return parser

class ThroughputTester:
    def __init__(self, nodes, pairs):
        self.nodes = nodes
        self.pairs = pairs

    def run_test(self):
        # Start time
        start_time = time.time()

        # Put all pairs
        for key, value in self.pairs.items():
            node = random.choice(self.nodes)
            self.put_value(node, key, value)

        # Get all pairs
        for key in self.pairs.keys():
            node = random.choice(self.nodes)
            ret_value = self.get_value(node, key)
            # Test correctness, minimal performance impact (data is in memory and stored in a dict)
            if ret_value != self.pairs[key]:
                raise Exception(f"Data mismatch for key {key}: expected {self.pairs[key]}, got {ret_value}")

        # End time
        end_time = time.time()
        duration = end_time - start_time
        total_operations = len(self.pairs) * 2  # Puts + Gets
        throughput = total_operations / duration

        return throughput
    
    def put_value(self, node, key, value):
        conn = None
        try:
            conn = http.client.HTTPConnection(node)
            conn.request("PUT", "/storage/"+key, value)
            conn.getresponse()
        finally:
            if conn:
                conn.close()

    def get_value(self, node, key):
        conn = None
        try:
            conn = http.client.HTTPConnection(node)
            conn.request("GET", "/storage/"+key)
            response = conn.getresponse()
            if response.status == 200:
                return response.read().decode("utf-8")
            else:
                return None
        finally:
            if conn:
                conn.close()

class PairGenerator:
    @staticmethod
    def generate_pairs(n):
        pairs = {}
        for _ in range(n):
            key = str(uuid.uuid4())
            value = ''.join(random.choices(string.ascii_letters + string.digits, k=20))
            pairs[key] = value
        return pairs
    
class ResultsCollector:
    def __init__(self):
        self.results = []

    def add_result(self, throughput, stddev):
        self.results.append((throughput, stddev))

    def print_results(self):
        print("Run\tThroughput (ops/sec)\tStdDev")
        for i, (throughput, stddev) in enumerate(self.results, 1):
            print(f"{i}\t{throughput:.2f}\t{stddev:.2f}")

def reset_node(node):
    """Reset a node to its initial state"""
    conn = None
    try:
        conn = http.client.HTTPConnection(node, timeout=10)
        conn.request("POST", "/reset")
        response = conn.getresponse()
        response.read()
        if response.status != 200:
            raise Exception(f"Failed to reset node {node}: {response.status}")
    finally:
        if conn:
            conn.close()

def recover_node(node):
    """Simulate recovery of a node"""
    conn = None
    try:
        conn = http.client.HTTPConnection(node, timeout=10)
        conn.request("POST", "/sim-recover")
        response = conn.getresponse()
        response.read()
        if response.status != 200:
            raise Exception(f"Failed to recover node {node}: {response.status}")
    finally:
        if conn:
            conn.close()

def join_node(node, nprime):
    """Join a node to the chord ring via nprime"""
    conn = None
    try:
        conn = http.client.HTTPConnection(node, timeout=10)
        conn.request("POST", f"/join?nprime={nprime}")
        response = conn.getresponse()
        response.read()
        if response.status != 200:
            raise Exception(f"Failed to join node {node} to {nprime}: {response.status}")
    finally:
        if conn:
            conn.close()

def get_node_info(node):
    """Get node info from a node"""
    conn = None
    try:
        conn = http.client.HTTPConnection(node, timeout=5)
        conn.request("GET", "/node-info")
        response = conn.getresponse()
        if response.status == 200:
            data = response.read().decode("utf-8")
            return json.loads(data)
        else:
            return None
    finally:
        if conn:
            conn.close()

def check_network_stability(nodes, timeout=30):
    """
    Check if the chord ring is stable by verifying that all nodes
    have consistent successor/predecessor relationships.
    """
    print(f"Checking network stability (timeout: {timeout}s)...")
    start_time = time.time()
    
    while time.time() - start_time < timeout:
        try:
            all_stable = True
            node_infos = {}
            
            # Collect info from all nodes
            for node in nodes:
                info = get_node_info(node)
                if info is None:
                    all_stable = False
                    break
                node_infos[node] = info
            
            if not all_stable:
                time.sleep(2)
                continue
            
            # Check if all nodes have valid successors and predecessors
            for node, info in node_infos.items():
                # Check if node has a successor
                if 'successor' not in info or info['successor'] is None:
                    all_stable = False
                    break
                
                # For single node, it's its own successor/predecessor
                if len(nodes) == 1:
                    continue
                
                # Check if node has a predecessor (for multi-node rings)
                if 'predecessor' not in info or info['predecessor'] is None:
                    all_stable = False
                    break
            
            if all_stable:
                print("✓ Network is stable!")
                return True
            
            time.sleep(2)
            
        except Exception as e:
            print(f"Error checking stability: {e}")
            time.sleep(2)
    
    print("✗ Network did not stabilize within timeout")
    return False

def setup_chord_ring(nodes):
    """
    Reset all nodes, recover them, and join them to form a chord ring.
    Returns True if successful, False otherwise.
    """
    if len(nodes) == 0:
        return False
    
    print(f"\n{'='*60}")
    print(f"Setting up Chord ring with {len(nodes)} nodes")
    print(f"{'='*60}\n")
    
    # Step 1: Recover and reset all nodes
    print("Step 1: Recovering and resetting all nodes...")
    for i, node in enumerate(nodes):
        try:
            print(f"  [{i+1}/{len(nodes)}] Recovering {node}...", end=" ")
            recover_node(node)
            print("✓", end=" ")
            print(f"Resetting...", end=" ")
            reset_node(node)
            print("✓")
        except Exception as e:
            print(f"✗ Error: {e}")
            return False
    
    print("✓ All nodes recovered and reset\n")
    
    # Step 2: Join all nodes to form a ring
    print("Step 2: Joining nodes to form Chord ring...")
    
    # First node is already in its own ring
    print(f"  [1/{len(nodes)}] {nodes[0]} is the initial node ✓")
    
    # Join remaining nodes to the ring
    for i, node in enumerate(nodes[1:], start=2):
        try:
            print(f"  [{i}/{len(nodes)}] Joining {node} via {nodes[0]}...", end=" ")
            join_node(node, nodes[0])
            print("✓")
            # Small delay to let stabilization happen
            time.sleep(0.5)
        except Exception as e:
            print(f"✗ Error: {e}")
            return False
    
    print("✓ All nodes joined\n")
    
    # Step 3: Wait for network to stabilize
    print("Step 3: Waiting for network stabilization...")
    if not check_network_stability(nodes, timeout=60):
        print("✗ Failed to stabilize network")
        return False
    
    print(f"\n{'='*60}")
    print("✓ Chord ring setup complete!")
    print(f"{'='*60}\n")
    
    return True
    

def test_throughput(node_list):
    node_count = 32
    pairs_per_test = 1000
    repetitions = 3

    results_collector = ResultsCollector()

    print(f"\n{'='*60}")
    print(f"Testing with {node_count} nodes")
    print(f"{'='*60}")
    throughputs = []
    
    for rep in range(repetitions):
        print(f"\nRepetition {rep+1}/{repetitions}")
        print("-" * 60)
        
        # Select nodes for this test
        selected_node_list = get_node_list(nodes=node_list, count=node_count)
        
        # Setup chord ring: recover, reset, and join
        if not setup_chord_ring(selected_node_list):
            print(f"✗ Failed to setup Chord ring for test")
            sys.exit(1)

        # Generate key-value pairs
        print(f"Running throughput test with {pairs_per_test} key-value pairs...")
        pairs = PairGenerator.generate_pairs(pairs_per_test)
        tester = ThroughputTester(selected_node_list, pairs)
        throughput = tester.run_test()
        throughputs.append(throughput)
        print(f"✓ Throughput: {throughput:.2f} ops/sec")
        
    avg_throughput = sum(throughputs) / len(throughputs)
    stddev_throughput = (sum((x - avg_throughput) ** 2 for x in throughputs) / len(throughputs)) ** 0.5
    results_collector.add_result(avg_throughput, stddev_throughput)
    print(f"\n✓ Average throughput for {node_count} nodes: {avg_throughput:.2f} ± {stddev_throughput:.2f} ops/sec")

    print(f"\n{'='*60}")
    print("All tests completed!")
    print(f"{'='*60}\n")
    results_collector.print_results()

def get_node_list(nodes, count):
    if count > len(nodes):
        raise ValueError("Not enough nodes provided")
    return random.sample(nodes, count)

def main(args):
    nodes = set(args.nodes)
    nodes = list(nodes)

    if len(nodes) < 32:
        print(f"✗ Error: Need at least 32 nodes to run the test, but only {len(nodes)} provided.")
        sys.exit(1)

    print(f"\n{'='*60}")
    print("CHORD DHT THROUGHPUT BENCHMARK")
    print(f"{'='*60}")
    print(f"Total nodes available: {len(nodes)}")
    print(f"Testing with: 32 nodes")
    print(f"{'='*60}\n")

    test_throughput(nodes)
    
    print(f"\n{'='*60}")
    print("✓ Throughput test completed!")
    print(f"{'='*60}\n")

if __name__ == "__main__":
    parser = arg_parser()
    args = parser.parse_args()
    main(args)