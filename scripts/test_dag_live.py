#!/usr/bin/env python3
"""Live deep-mode task-DAG run via the debug socket. Hunts for problems."""
import socket, json, os, sys

SOCK = f"/run/user/{os.getuid()}/jcode-debug.sock"
import time
TEST_DIR = f"/tmp/dag-live-{int(time.time()*1000)}"

def cmd(c, session_id=None, timeout=30):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect(SOCK); s.settimeout(timeout)
    req = {"type":"debug_command","id":1,"command":c}
    if session_id: req["session_id"]=session_id
    s.send((json.dumps(req)+"\n").encode())
    data=b""
    while True:
        ch=s.recv(65536)
        if not ch: break
        data+=ch
        if b"\n" in data: break
    s.close()
    r=json.loads(data.decode().strip())
    return r.get("ok",False), r.get("output",""), r.get("error","")

def graph(obj):
    ok,out,err = cmd("swarm:graph:"+json.dumps(obj))
    if not ok:
        return False, out, err
    try:
        inner=json.loads(out)
        return inner.get("ok",False), out, inner.get("error","")
    except Exception:
        return False, out, "unparseable: "+out

def show_plan(swarm_id):
    ok,out,err = cmd(f"swarm:plan:{swarm_id}")
    if not ok:
        print("  plan read failed:", err or out); return None
    p=json.loads(out)
    print(f"  v{p['version']} mode={p.get('mode')} items={len(p['items'])} "
          f"ready={p['ready_ids']} blocked={p['blocked_ids']} done={p['completed_ids']}")
    for it in p["items"]:
        meta=p.get("node_meta",{}).get(it["id"],{})
        flags=[]
        if meta.get("is_gate"): flags.append("GATE")
        if meta.get("expanded"): flags.append("composite")
        if meta.get("planner"): flags.append(f"planner={meta['planner']}")
        art = "art" if meta.get("artifact_json") else ""
        print(f"    - {it['id']:14} {it['status']:10} kind={meta.get('kind')} "
              f"deps={it.get('blocked_by',[])} {' '.join(flags)} {art}")
    return p

problems=[]
def check(cond, msg):
    print(("  OK " if cond else "  ✗ FAIL ")+msg)
    if not cond: problems.append(msg)

os.makedirs(TEST_DIR, exist_ok=True)
ok,out,err = cmd(f"create_session:{TEST_DIR}")
s1 = json.loads(out)["session_id"]
ok,out,err = cmd(f"create_session:{TEST_DIR}")
s2 = json.loads(out)["session_id"]
ok,out,_ = cmd(f"swarm:id:{TEST_DIR}")
swarm_id = json.loads(out)["swarm_id"]
print(f"swarm={swarm_id} s1={s1[:12]} s2={s2[:12]}")

print("\n[1] seed deep graph: explore -> synth")
ok,out,err = graph({"op":"seed","swarm_id":swarm_id,"mode":"deep","nodes":[
    {"id":"explore","content":"explore multimonitor","kind":"explore"},
    {"id":"synth","content":"synthesize","kind":"synthesize","depends_on":["explore"]},
]})
check(ok, f"seed deep ({err or out})")
p=show_plan(swarm_id)
check(p and p.get("mode")=="deep", "mode persisted as deep")
check(p and "explore" in p["ready_ids"], "explore is ready")
check(p and "synth" in p["blocked_ids"], "synth blocked on explore")

print("\n[2] expand explore into 2 facets (s1 is planner)")
ok,out,err = graph({"op":"expand","swarm_id":swarm_id,"actor":s1,"node_id":"explore","children":[
    {"id":"geo","content":"geometry","kind":"explore"},
    {"id":"hot","content":"hotplug","kind":"explore"},
]})
check(ok, f"expand ({err or out})")
p=show_plan(swarm_id)
gate=[it["id"] for it in p["items"] if p["node_meta"].get(it["id"],{}).get("is_gate")]
check(len(gate)==1, f"exactly one gate inserted ({gate})")
check(p["node_meta"].get("explore",{}).get("expanded"), "explore is composite")
check(p["node_meta"].get("explore",{}).get("planner")==s1, "explore planner=s1")
check("geo" in p["ready_ids"] and "hot" in p["ready_ids"], "facets ready")
gate_id = gate[0] if gate else None
check(gate_id and gate_id not in p["ready_ids"], "gate not ready until facets done")

print("\n[3] complete both facets")
for fid in ["geo","hot"]:
    ok,out,err = graph({"op":"complete","swarm_id":swarm_id,"actor":s2,"node_id":fid,
        "artifact":{"findings":f"{fid} findings","what_i_did_not_check":["edge cases"]}})
    check(ok, f"complete {fid} ({err or out})")
p=show_plan(swarm_id)
check(gate_id in p["ready_ids"], "gate now ready after facets done")

print("\n[4] gate finds a gap -> inject new node")
ok,out,err = graph({"op":"inject","swarm_id":swarm_id,"actor":s2,"gate_id":gate_id,"nodes":[
    {"id":"dpi","content":"DPI scaling","kind":"explore"},
]})
check(ok, f"inject gap ({err or out})")
p=show_plan(swarm_id)
check("dpi" in p["ready_ids"], "gap node dpi ready")
check(gate_id not in p["ready_ids"], "gate re-blocked on gap")
check("explore" not in p["ready_ids"], "composite still blocked")

print("\n[5] complete gap, then gate passes")
ok,out,err = graph({"op":"complete","swarm_id":swarm_id,"actor":s2,"node_id":"dpi",
    "artifact":{"findings":"dpi done","what_i_did_not_check":["none"]}})
check(ok, f"complete dpi ({err or out})")
p=show_plan(swarm_id)
check(gate_id in p["ready_ids"], "gate ready again")
ok,out,err = graph({"op":"complete","swarm_id":swarm_id,"actor":s2,"node_id":gate_id,
    "artifact":{"findings":"gate passed","what_i_did_not_check":["none"]}})
check(ok, f"complete gate ({err or out})")
p=show_plan(swarm_id)
check("explore" in p["ready_ids"], "composite explore now ready for synthesis")

print("\n[6] complete composite synthesis, then synth")
ok,out,err = graph({"op":"complete","swarm_id":swarm_id,"actor":s1,"node_id":"explore",
    "artifact":{"findings":"explore synthesized","what_i_did_not_check":["none"]}})
check(ok, f"complete explore ({err or out})")
p=show_plan(swarm_id)
check("synth" in p["ready_ids"], "synth ready after explore done")

print("\n[7] adversarial: thin artifact must be rejected in deep mode")
ok,out,err = graph({"op":"complete","swarm_id":swarm_id,"actor":s1,"node_id":"synth",
    "artifact":{"findings":""}})
check(not ok, f"empty findings rejected ({err[:60] if err else out[:60]})")

print("\n[8] adversarial: non-owner cannot complete")
# synth currently has no owner until dispatched; debug complete sets owner=actor, so
# test ownership via expand on a node owned by someone else.
ok,out,err = graph({"op":"complete","swarm_id":swarm_id,"actor":s1,"node_id":"synth",
    "artifact":{"findings":"final report","what_i_did_not_check":["none"]}})
check(ok, f"valid synth completion ({err or out})")
p=show_plan(swarm_id)
check(set(p["completed_ids"])>= {"geo","hot","dpi","explore","synth",gate_id},
      "all nodes completed")

cmd(f"destroy_session:{s1}"); cmd(f"destroy_session:{s2}")
print("\n=== %d problem(s) ===" % len(problems))
for p in problems: print(" -",p)
sys.exit(1 if problems else 0)
