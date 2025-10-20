import sys
import requests
import time

def join_ring(new_node, existing_ring_node):
    response = requests.post(f"http://{new_node}/join?nprime={existing_ring_node}")
    print(f"{new_node} JOIN response: {response.status_code}.")

def get_info(node):
    response = requests.get(f"http://{node}/node-info")
    info = {}
    if response.status_code == 200:
        info = response.json()
    return info

if len(sys.argv) < 2:
    print("Usage: python3 join_experiment.py <host:port> [<host:port> ...]")
    sys.exit(1)

host_ports = sys.argv[1:]
graceful_test = True # Pauses for one second between each request for nodes to join the network. Set to false for proper benchmarking
max_test_time = 120 # time in seconds before experiment times out

print("Received host:port pairs:", host_ports)

for node in host_ports[1:]:
    join_ring(node, host_ports[0])
    if graceful_test:
        time.sleep(1)

unique_nodes_in_ring = set()
unique_nodes_in_ring.add(host_ports[0])
current_node = host_ports[0]
start_time = time.time()
end_time = start_time + max_test_time

while len(unique_nodes_in_ring) < len(host_ports) and time.time() < end_time:
    info = get_info(current_node)
    if "successor" in info:
        unique_nodes_in_ring.add(info["successor"])
        current_node = info["successor"]

if len(unique_nodes_in_ring) == len(host_ports):
    print("All nodes have successfully joined the ring.")
    print("Elapsed time:", time.time() - start_time, "seconds")
else:
    print("Some nodes failed to join the ring.")
    print(f"Nodes in ring: {unique_nodes_in_ring} / {len(host_ports)}")
    print(unique_nodes_in_ring)
    print("Elapsed time:", time.time() - start_time, "seconds")