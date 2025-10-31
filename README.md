# inf3200

This repository contains code for INF3200 Assignment 2. It consists of two rust projects, and bash scripts to download and run the servers on a cluster. 

It is compiled through GitHub Actions, and is provided in x86_64 Linux binary format (MSUL over GNU for better portability).

## Webserver (``src/webserver``)

The webserver is built on actix-web, and provides a simple HTTP server that exposes the endpoints necessary for a DHT (distributed hash table) using the Chord Protocol.
The server uses middleware to track the last activity on this node. After x minutes of inactivty, it automatically shuts down (currently configured to 10 minutes).

## Deploy (``src/deploy``)

The deploy script gets a list of available nodes from the cluster, and start x number of servers on different nodes (by downloading and executing the run-node.sh script on each node). It picks a random port and checks if it is available before starting the server. If more servers are requested than available nodes, it will wrap around and start on nodes that already have a server running. After it has started x servers and confirmed that they are alive, it initializes the DHT on all nodes by sending the full list of all nodes in the cluster.

## Healthcheck test (``src/tests/health_check.py``)

Takes a list of nodes and checks that all of them are up and reachable by pinging the /hello-world and /node-info endpoints.

## Chord benchmark (``src/tests/chord_benchmark.py``)

Takes a list of nodes (preferably 32), and runs the following sets of experiments on them:
1. Network growth. Checks that a stable ring can be reached from joining 2, 4, 8, 16, and 32 single network nodes together.
2. Network shrinking. Checks that a stable ring can be reached when shrinking a network from 32 -> 16 nodes, 16 -> 8 nodes, 8 -> 4 nodes,  and 4 -> 2 nodes.
3. Crash recovery. Checks that a stable ring can be reached when 1-16 nodes crash simultaneously (from a 32 node ring).

Takes a repetitions flag, if not provided repetitions are set to 3. In between each repetition we do a hard reset off all nodes to prevent any leftover state.

## Throughput test (``src/tests/throughput.py``)

The throughput test script checks that our chord network is still able to maintain data integrity for PUT and GET calls to the storage endpoint when using dynamic joining. It sets up a chord ring of 32 nodes (needs 32 nodes to be provided) using join, and runs data integrity checks on the network. Note: We don't test data integrity before and after leaving or crashing as data will be lost in the current implementation, correctness after shrinking is also tricky as it takes some time for the finger tables to correct themselves (we're also not 100% sure that the finger tables *actually* become completely correct for a large node network as the assignment did not require testing of this, we checked it manually for a small node network of 8 nodes and it seemed to be correct).

## Downloading and running
1. Login to the cluster
2. Download the bash file:
   ```bash
   wget https://github.com/augusthindenes/inf3200/releases/download/v0.2.15/run.sh
   ```
3. Make it executable:
   ```bash
   chmod +x run.sh
   ```
4. Run the script with the number of servers to start as an argument:
   ```bash
   ./run.sh <number of servers>
   ```
   (Number of servers is limited to 100)
5. Check that the servers are running: (Optional but recommended)
   ```bash
   python3 health_check.py [output from run.sh (plaintext format, not JSON)]
   ```
   
   If needed, download health_check:
   ```bash
   wget https://github.com/augusthindenes/inf3200/releases/download/v0.2.15/health_check.py
   ```
6. Run chord benchmark
   ```bash
   python3 chord_benchmark.py [output from run.sh (plaintext format, not JSON)]
   ```

   If needed, download chord_tester:
   ```bash
   wget https://github.com/augusthindenes/inf3200/releases/download/v0.2.15/chord_benchmark.py
   ```
7. Run Throughput tester
   ```bash
   python3 throughput.py [output from run.sh (plaintext format, not JSON)]
   ```

   If needed, download throughput tester:
   ```bash
   wget https://github.com/augusthindenes/inf3200/releases/download/v0.2.15/throughput.py
   ```
8. Stop all servers and exit the cluster:
   ```bash
   /share/ifi/cleanup.sh
   ```

## Example: Downloading run.sh, starting a 32 node cluster and running the chord_benchmark test.

1. Download run.sh and chord_benchmark.py
   ```bash
   wget https://github.com/augusthindenes/inf3200/releases/download/v0.2.15/run.sh
   wget https://github.com/augusthindenes/inf3200/releases/download/v0.2.15/chord_benchmark.py
   ```
2. Set execution permissions for the run script
   ```bash
   chmod +x run.sh
   ```
3. Start 32 nodes
   ```bash
   ./run.sh 32
   ```
4. Wait for the run script to finish
   You will see the following in your terminal:
   ```bash
   (Node list in json, ignore this)
   ...
   Node list (plain text for other test scripts):
   node1:1234 node2:2345 node3:3456 ... node32:9999
   ```
   Copy the plaintext list of nodes
5. Start chord_benchmark test
   ```bash
   python3 chord_benchmark.py node1:1234 node2:2345 node3:3456 ... node32:9999
   ```
   Results will be printed to the terminal, and when finished PDF graphs will be written to ...
6. Cleanup
   ```bash
   /share/ifi/cleanup.sh
   ```
