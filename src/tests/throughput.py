# We want to test the throughput of our Chord DHT. There are two interesting parameters:
# 1) The number of nodes in the DHT
# 2) The size of the finger table (M)
# We want to see how these parameters affect the throughput of the DHT.
# Each test will be run 3 times to get an average (with a standard deviation).
# Finally we will plot the results using matplotlib.

# Node counts:
# 1, 2, 4, 8, 16, 32
# M values:
# 0, 2, 4, 6, 8 (1 instead of 0 as it's just the successor anyways)
# Each combination of node count and M value will be tested.
# Each test will consist of putting and getting 1000 key-value pairs.

import http.client
import json
import random
import string
import time
import uuid
import matplotlib.pyplot as plt
import argparse

def arg_parser():
    parser = argparse.ArgumentParser(prog="client", description="DHT client")

    parser.add_argument("nodes", type=str, nargs="+",
            help="addresses (host:port) of nodes to test")

    return parser

class ThroughputTester:
    def __init__(self, nodes, m, pairs):
        self.nodes = nodes
        self.m = m
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

    def add_result(self, nodes, m, throughput, stddev):
        self.results.append((nodes, m, throughput, stddev))

    def print_results(self):
        print("Nodes\tM\tThroughput (ops/sec)\tStdDev")
        for nodes, m, throughput, stddev in self.results:
            print(f"{nodes}\t{m}\t{throughput:.2f}\t{stddev:.2f}")

def reconfigure_nodes(nodes, m):
    for node in nodes:
        conn = None
        try:
            conn = http.client.HTTPConnection(node)
            # Body is a JSON with nodes=list of nodes max_nodes=len(nodes) finger_table_size=m
            body = {
                "nodes": nodes,
                "max_nodes": len(nodes),
                "finger_table_size": m
            }
            headers = {"Content-Type": "application/json"}
            conn.request("POST", f"/reconfigure", json.dumps(body), headers)
            response = conn.getresponse()
            if response.status != 200:
                raise Exception(f"Failed to reconfigure node {node}: {response.status}")
        finally:
            if conn:
                conn.close()
    
def test_throughput(node_list):
    node_counts = [1, 2, 4, 8, 16, 32]
    m_values = [0, 1, 2, 4, 8]
    pairs_per_test = 1000
    repetitions = 3

    results_collector = ResultsCollector()

    for nodes in node_counts:
        for m in m_values:
            print(f"Testing with {nodes} nodes and M={m}")
            throughputs = []
            for _ in range(repetitions):
                # Assume we have a function to get the list of nodes
                selected_node_list = get_node_list(nodes=node_list, count=nodes)
                # Reconfigure nodes to use N nodes and M finger table size
                reconfigure_nodes(selected_node_list, m)

                # Generate key-value pairs
                pairs = PairGenerator.generate_pairs(pairs_per_test)
                tester = ThroughputTester(selected_node_list, m, pairs)
                throughput = tester.run_test()
                throughputs.append(throughput)
            avg_throughput = sum(throughputs) / len(throughputs)
            stddev_throughput = (sum((x - avg_throughput) ** 2 for x in throughputs) / len(throughputs)) ** 0.5
            results_collector.add_result(nodes, m, avg_throughput, stddev_throughput)

    results_collector.print_results()

    # Use matplotlib to plot results with error bars showing stddev
    for m in m_values:
        entries = [r for r in results_collector.results if r[1] == m]
        x = [r[0] for r in entries]
        y = [r[2] for r in entries]
        yerr = [r[3] for r in entries]
        plt.errorbar(x, y, yerr=yerr, marker='o', capsize=5, label=f'M={m}')
    plt.xlabel('Number of Nodes')
    plt.ylabel('Throughput (ops/sec)')
    plt.title('DHT Throughput vs Number of Nodes')
    plt.legend()
    plt.grid(True)
    plt.savefig('throughput.pdf')

    # Make a plot of lookup time aswell
    plt.clf()
    for m in m_values:
        entries = [r for r in results_collector.results if r[1] == m]
        x = [r[0] for r in entries]
        y = [r[0]/r[2] for r in entries]  # Average time per operation
        yerr = [r[3]/(r[2]**2) * r[0] for r in entries]  # Error propagation
        plt.errorbar(x, y, yerr=yerr, marker='o', capsize=5, label=f'M={m}')
    plt.xlabel('Number of Nodes')
    plt.ylabel('Average Time per Operation (sec)')
    plt.title('DHT Average Time per Operation vs Number of Nodes')
    plt.legend()
    plt.grid(True)
    plt.savefig('lookup_time.pdf')

def get_node_list(nodes, count):
    if count > len(nodes):
        raise ValueError("Not enough nodes provided")
    return random.sample(nodes, count)

def main(args):
    nodes = set(args.nodes)
    nodes = list(nodes)

    test_throughput(nodes)
    print("Throughput test completed. Results saved to throughput.png")

if __name__ == "__main__":
    parser = arg_parser()
    args = parser.parse_args()
    main(args)