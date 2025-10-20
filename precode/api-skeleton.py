from flask import Flask, request, jsonify
import sys

app = Flask(__name__)

kv = {}

node_hash = "example_node_hash"
successor = "example_successor:10000"
finger_table = ["example_successor:10001", "example_successor:10002"]

@app.route('/storage/<key>', methods=['GET'])
def handle_get(key):
    val = kv.get(key, None)
    if val is None:
        return "Not Found", 404, {"Content-type": "text/plain"}
    else:
        return val, 200, {"Content-type": "text/plain"}

@app.route('/storage/<key>', methods=['PUT'])
def handle_storage_put(key):
    data = request.get_data(as_text=True)
    kv[key] = data
    return "OK", 200, {"Content-type": "text/plain"}

@app.route('/node-info', methods=['GET'])
def handle_node_info():
    info = {
        "node_hash": node_hash,
        "successor": successor,
        "others": finger_table
    }
    return jsonify(info), 200, {"Content-type": "application/json"}

@app.route('/leave', methods=['POST'])
def handle_leave():
    return "OK", 200, {"Content-type": "text/plain"}

@app.route('/sim-crash', methods=['POST'])
def handle_sim_crash():
    return "OK", 200, {"Content-type": "text/plain"}

@app.route('/sim-recover', methods=['POST'])
def handle_sim_recover():
    return "OK", 200, {"Content-type": "text/plain"}

@app.route('/join', methods=['POST'])
def handle_join():
    node = request.args.get('nprime')
    return "OK", 200, {"Content-type": "text/plain"}

if __name__ == '__main__':
    if len(sys.argv) != 2:
        print("Missing argument: <host:port>")
        sys.exit(1)

    host, port = sys.argv[1].split(':')
    app.run(host=host, port=int(port))
