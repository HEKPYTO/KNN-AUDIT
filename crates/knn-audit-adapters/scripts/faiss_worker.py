import json
import sys


try:
    import faiss
    import numpy as np

    IMPORT_ERROR = None
except ImportError as exc:
    faiss = None
    np = None
    IMPORT_ERROR = (
        f"faiss is not installed: {exc}; install with: "
        f"{sys.executable} -m pip install 'faiss-cpu==1.12.0'"
    )

state = {"index": None, "ids": []}


def require_index():
    if state["index"] is None:
        raise ValueError("index is not created")
    return state["index"]


def handle(request):
    if not isinstance(request, dict):
        raise ValueError("request must be a JSON object")
    op = request.get("op")
    if IMPORT_ERROR is not None:
        raise RuntimeError(IMPORT_ERROR)
    if op == "version":
        return {"version": str(faiss.__version__)}
    if op == "create":
        dim = request.get("dim")
        if not isinstance(dim, int) or isinstance(dim, bool) or dim <= 0:
            raise ValueError("dim must be a positive integer")
        state["index"] = faiss.IndexFlatL2(dim)
        state["ids"] = []
        return {"ok": True}
    if op == "add":
        index = require_index()
        ids = request.get("ids")
        vectors = request.get("vectors")
        if not isinstance(ids, list) or not isinstance(vectors, list):
            raise ValueError("ids and vectors must be arrays")
        dim = index.d
        if len(vectors) != len(ids) * dim:
            raise ValueError(f"vectors has {len(vectors)} values, expected {len(ids) * dim}")
        if not ids:
            return {"ok": True}
        array = np.asarray(vectors, dtype=np.float32).reshape(len(ids), dim)
        index.add(array)
        state["ids"].extend(ids)
        return {"ok": True}
    if op == "count":
        return {"count": require_index().ntotal}
    if op == "search":
        index = require_index()
        query = request.get("query")
        k = request.get("k")
        if not isinstance(query, list) or len(query) != index.d:
            raise ValueError(f"query must have {index.d} values")
        if not isinstance(k, int) or isinstance(k, bool) or k <= 0:
            raise ValueError("k must be a positive integer")
        k_eff = min(k, index.ntotal)
        if k_eff == 0:
            return {"hits": []}
        array = np.asarray(query, dtype=np.float32).reshape(1, index.d)
        distances, labels = index.search(array, k_eff)
        ids = state["ids"]
        hits = []
        for label, distance in zip(labels[0], distances[0]):
            position = int(label)
            if position < 0:
                raise ValueError(f"f32 squared-L2 overflow: faiss returned label {position}")
            if position >= len(ids):
                raise ValueError(f"faiss returned unknown index {position}")
            hits.append({"id": ids[position], "distance": float(distance)})
        return {"hits": hits}
    raise ValueError(f"unknown op {op!r}")


for line in sys.stdin:
    try:
        response = handle(json.loads(line))
    except Exception as exc:
        response = {"error": str(exc)}
    sys.stdout.write(json.dumps(response) + "\n")
    sys.stdout.flush()
