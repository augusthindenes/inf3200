import argparse
import http.client
import sys
from typing import List, Tuple


def check_node(address: str) -> Tuple[bool, str]:
    try:
        conn = http.client.HTTPConnection(address, timeout=5)
        
        # Test /helloworld endpoint
        conn.request("GET", "/helloworld")
        response = conn.getresponse()
        
        if response.status != 200:
            return False, f"❌ /helloworld returned {response.status}"
        
        response.read()  # Clear the response
        
        # Test /node-info endpoint
        conn.request("GET", "/node-info")
        response = conn.getresponse()
        
        if response.status != 200:
            return False, f"❌ /node-info returned {response.status}"
        
        response.read()
        conn.close()
        
        return True, "✓ All endpoints working"
        
    except Exception as e:
        return False, f"❌ Connection failed: {e}"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "nodes",
        type=str,
        nargs="+",
        help="Node addresses (host:port) to check"
    )
    
    args = parser.parse_args()
    node_addresses = list(set(args.nodes))  # Remove duplicates
    
    print("=" * 80)
    print("CHORD NODE HEALTH CHECK")
    print("=" * 80)
    print(f"Checking {len(node_addresses)} nodes...\n")
    
    results = []
    for i, address in enumerate(node_addresses, 1):
        print(f"[{i}/{len(node_addresses)}] {address}...", end=" ", flush=True)
        success, message = check_node(address)
        results.append((address, success, message))
        print(message)
    
    # Summary
    print("\n" + "=" * 80)
    print("SUMMARY")
    print("=" * 80)
    
    healthy = [r for r in results if r[1]]
    unhealthy = [r for r in results if not r[1]]
    
    print(f"Healthy nodes: {len(healthy)}/{len(node_addresses)}")
    
    if unhealthy:
        print(f"\nUnhealthy nodes ({len(unhealthy)}):")
        for address, _, message in unhealthy:
            print(f"  {address}: {message}")
        print("\n!  Some nodes are not responding correctly.")
        print("Please fix these issues before running benchmarks.")
        sys.exit(1)
    else:
        print("\n✓ All nodes are healthy and ready for benchmarking!")
        print(f"\nYou can now run:")
        print(f"  ./chord_benchmark.py {' '.join(node_addresses[:5])}{'...' if len(node_addresses) > 5 else ''}")
        sys.exit(0)


if __name__ == "__main__":
    main()
