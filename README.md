# inf3200

This repository contains code for INF3200 Assignment 2. It consists of two rust projects, and bash scripts to download and run the servers on a cluster. 

It is compiled through GitHub Actions, and is provided in x86_64 Linux binary format (MSUL over GNU for better portability).

## Webserver (``src/webserver``)

The webserver is built on actix-web, and provides a simple HTTP server that exposes the endpoints necessary for a DHT (distributed hash table) using the Chord Protocol.
The server uses middleware to track the last activity on this node. After x minutes of inactivty, it automatically shuts down (currently configured to 10 minutes).

## Deploy (``src/deploy``)

The deploy script gets a list of available nodes from the cluster, and start x number of servers on different nodes (by downloading and executing the run-node.sh script on each node). It picks a random port and checks if it is available before starting the server. If more servers are requested than available nodes, it will wrap around and start on nodes that already have a server running. After it has started x servers and confirmed that they are alive, it initializes the DHT on all nodes by sending the full list of all nodes in the cluster.

## Throughput test (``src/tests/throughput.py``)

The throughput test script checks that our chord network is still able to 

## Downloading and running
1. Login to the cluster
2. Download the bash file:
   ```bash
   wget https://github.com/augusthindenes/inf3200/releases/download/v0.2.13/run.sh
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
   wget https://github.com/augusthindenes/inf3200/releases/download/v0.2.13/health_check.py
   ```
6. Run chord benchmark
   ```bash
   python3 chord_benchmark.py [output from run.sh (plaintext format, not JSON)]
   ```

   If needed, download chord_tester:
   ```bash
   wget https://github.com/augusthindenes/inf3200/releases/download/v0.2.13/chord_benchmark.py
   ```
7. Run Throughput tester
   ```bash
   python3 throughput.py [output from run.sh (plaintext format, not JSON)]
   ```

   If needed, download throughput tester:
   ```bash
   wget https://github.com/augusthindenes/inf3200/releases/download/v0.2.13/throughput.py
   ```
8. Stop all servers and exit the cluster:
   ```bash
   /share/ifi/cleanup.sh
   ```

## Example: Downloading run.sh, starting a 32 node cluster and running the chord_benchmark test.

1. Download run.sh and throughput.py
   ```bash
   wget https://github.com/augusthindenes/inf3200/releases/download/v0.2.13/run.sh
   wget https://github.com/augusthindenes/inf3200/releases/download/v0.2.13/chord_benchmark.py
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

## OpenAPI Specification for Webserver

<details>
<summary>OpenAPI (click to expand)</summary>

```yaml
openapi: 3.0.3
info:
  title: Chord DHT Node API
  version: 1.0.0
  description: |
    HTTP API for a single node in a Chord-based distributed hash table (Actix-web).
servers:
  - url: http://{host}:{port}
    variables:
      host:
        default: localhost
      port:
        default: "8080"
paths:
  /helloworld:
    get:
      tags: [Health]
      summary: Return node bind address
      description: Returns the hostname and port this node is running on in the format "host:port".
      responses:
        '200':
          description: OK
          content:
            text/plain:
              schema:
                type: string
                example: "localhost:8080"

  /storage/{key}:
    get:
      tags: [Storage]
      summary: Get a value
      description: |
        Retrieves the value for **key**. If the current node isn’t responsible for the key, the request may be forwarded through the Chord ring.
      parameters:
        - $ref: '#/components/parameters/KeyPath'
        - $ref: '#/components/parameters/HopCountHeader'
      responses:
        '200':
          description: Value found
          content:
            text/plain:
              schema:
                type: string
        '404':
          description: Key not found
          content:
            text/plain:
              schema:
                type: string
                example: "Key not found"
        '502':
          description: Error forwarding request to the responsible node
          content:
            text/plain:
              schema:
                type: string
                example: "Error forwarding request"
        '503':
          description: Distributed Hashtable not initialized
          content:
            text/plain:
              schema:
                type: string
                example: "Distributed Hashtable not initialized"
    put:
      tags: [Storage]
      summary: Put a value
      description: |
        Stores a UTF-8 string **value** under **key**. If the current node isn’t responsible for the key, the request may be forwarded.
      parameters:
        - $ref: '#/components/parameters/KeyPath'
        - $ref: '#/components/parameters/HopCountHeader'
      requestBody:
        required: true
        content:
          text/plain:
            schema:
              type: string
            examples:
              example1:
                value: "some value"
      responses:
        '200':
          description: Stored
          content:
            text/plain:
              schema:
                type: string
                example: "Value stored"
        '400':
          description: Value must be valid UTF-8
          content:
            text/plain:
              schema:
                type: string
                example: "Value must be valid UTF-8"
        '502':
          description: Error forwarding request to the responsible node
          content:
            text/plain:
              schema:
                type: string
                example: "Error forwarding request"
        '503':
          description: Distributed Hashtable not initialized
          content:
            text/plain:
              schema:
                type: string
                example: "Distributed Hashtable not initialized"

  /network:
    get:
      tags: [Network]
      summary: Get known nodes
      description: Returns the list of nodes known to this node.
      responses:
        '200':
          description: OK
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/KnownNodesResponse'
        '503':
          description: Distributed Hashtable not initialized
          content:
            text/plain:
              schema:
                type: string
                example: "Distributed Hashtable not initialized"

  /storage-init:
    post:
      tags: [Admin]
      summary: Initialize this node
      description: |
        Initializes the node and joins/creates the ring. The **nodes** list **must include this node** (`host:port`).
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/InitReq'
            examples:
              example1:
                value:
                  nodes: ["localhost:8080", "peer1:8080", "peer2:8080"]
      responses:
        '200':
          description: Node initialized
          content:
            text/plain:
              schema:
                type: string
                example: "Node initialized"
        '400':
          description: |
            Bad request — either the node is already initialized or the initialization list didn’t include this node.
          content:
            text/plain:
              schema:
                type: string
                examples:
                  alreadyInitialized:
                    value: "Node already initialized"
                  missingSelf:
                    value: "Initialization list must include this node"

  /reconfigure:
    post:
      tags: [Admin]
      summary: Reconfigure the ring membership/parameters
      description: |
        Reinitializes the node with a new node list and optional parameters. The **nodes** list **must include this node**.
        Storage is reset in this implementation (data redistribution is not performed).
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/ReconfigReq'
            examples:
              example1:
                value:
                  nodes: ["localhost:8080", "peer1:8080"]
                  max_nodes: 64
                  finger_table_size: 32
      responses:
        '200':
          description: Node reconfigured
          content:
            text/plain:
              schema:
                type: string
                example: "Node reconfigured"
        '400':
          description: Reconfiguration list must include this node
          content:
            text/plain:
              schema:
                type: string
                example: "Reconfiguration list must include this node"
        '503':
          description: Distributed Hashtable not initialized
          content:
            text/plain:
              schema:
                type: string
                example: "Distributed Hashtable not initialized"

components:
  parameters:
    KeyPath:
      name: key
      in: path
      required: true
      description: Key to read/write
      schema:
        type: string
    HopCountHeader:
      name: X-Chord-Hop-Count
      in: header
      required: false
      description: Number of hops already taken when forwarding through the ring (used internally).
      schema:
        type: integer
        minimum: 0
        example: 0

  schemas:
    NodeAddr:
      type: object
      properties:
        host:
          type: string
          example: "localhost"
        port:
          type: integer
          format: int32
          example: 8080
      required: [host, port]

    KnownNodesResponse:
      type: array
      items:
        $ref: '#/components/schemas/NodeAddr'

    InitReq:
      type: object
      properties:
        nodes:
          type: array
          description: List of nodes in "host:port" form. **Must include this node.**
          items:
            type: string
            example: "localhost:8080"
      required: [nodes]

    ReconfigReq:
      type: object
      properties:
        nodes:
          type: array
          description: List of nodes in "host:port" form. **Must include this node.**
          items:
            type: string
            example: "localhost:8080"
        max_nodes:
          type: integer
          minimum: 1
          description: Optional maximum number of nodes to keep.
          example: 64
        finger_table_size:
          type: integer
          minimum: 1
          description: Optional finger table size.
          example: 32
      required: [nodes]
tags:
  - name: Health
  - name: Storage
  - name: Network
  - name: Admin
```
</details>
