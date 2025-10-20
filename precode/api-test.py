import requests
import sys

BASE = ""

def test_get_storage():
    r = requests.get(f"http://{BASE}/storage/testkey")
    print("GET /storage/testkey:", r.status_code, r.text)
    assert r.status_code == 200, "get returned not 200"

def test_put_storage():
    r = requests.put(f"http://{BASE}/storage/testkey", data="testvalue")
    print("PUT /storage/testkey:", r.status_code, r.text)
    assert r.status_code == 200, "put returned not 200"

def test_node_info():
    r = requests.get(f"http://{BASE}/node-info")
    try:
        info = r.json()
        node_hash = info.get("node_hash")
        successor = info.get("successor")
        finger_table = info.get("others")

        assert node_hash is not None, "node_hash is None"
        assert successor is not None, "successor is None"
        assert finger_table is not None, "finger_table is None"

    except ValueError:
        print("Failed to parse JSON response")
        
    print("GET /node-info:", r.status_code, r.headers.get("Content-type"), r.text)
    assert r.status_code == 200, "node-info returned not 200"

def test_leave():
    r = requests.post(f"http://{BASE}/leave")
    print("POST /leave:", r.status_code, r.text)
    assert r.status_code == 200, "leave returned not 200"

def test_sim_crash():
    r = requests.post(f"http://{BASE}/sim-crash")
    print("POST /sim-crash:", r.status_code, r.text)
    assert r.status_code == 200, "sim-crash returned not 200"

def test_sim_recover():
    r = requests.post(f"http://{BASE}/sim-recover")
    print("POST /sim-recover:", r.status_code, r.text)
    assert r.status_code == 200, "sim-recover returned not 200"

def test_join():
    r = requests.post(f"http://{BASE}/join?nprime=localhost:10001")
    print("POST /join?nprime=localhost:10001:", r.status_code, r.text)
    assert r.status_code == 200, "join returned not 200"

def run_tests():
    tests = [test_put_storage, test_get_storage, test_node_info,
             test_leave, test_sim_crash, test_sim_recover, test_join]
    
    for test in tests:
        print("Running test...", test.__name__)
        try:
            test()
        except AssertionError as e:
            print(f"FAILED:     Test {test.__name__} failed: {e}")
        except Exception as e:
            print(f"FAILED:     Test {test.__name__} encountered an error: {e}")
        else:
            print(f"Passed!:    Test {test.__name__} passed.")
        print("\n-----\n")

    print("Note that this only tests the correctness of the HTTP API, not the Chord application logic.")


if __name__ == "__main__":
    if len(sys.argv) > 1:
        BASE = sys.argv[1]
    else:
        print("Missing argument: <host:port>")
        sys.exit(1)

    run_tests()