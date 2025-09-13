# inf3200

This repository contains code for INF3200 Assignment 1A. It consists of two rust projects, and bash scripts to download and run the servers on a cluster. 

It is compiled through GitHub Actions, and is provided in x86_64 Linux binary format (MSUL over GNU for better portability).

The webserver is built on actix-web, and provides a simple HTTP server that responds with its node name and port (provided as command line arguments at execution).

The deploy script gets a list of available nodes from the cluster, and start x number of servers on different nodes (by downloading and executing the run-node.sh script on each node). It picks a random port and checks if it is available before starting the server. If more servers are requested than available nodes, it will wrap around and start on nodes that already have a server running.

## Downloading and running
1. Login to the cluster
2. Download the bash file:
   ```bash
   wget https://github.com/augusthindenes/inf3200/releases/download/v0.1.1/run.sh
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
5. Check that the servers are running:
   ```bash
   python3 testscript.py [output from run.sh]
   ```
6. Stop all servers and exit the cluster:
   ```bash
   /share/ifi/cleanup.sh
   ```
