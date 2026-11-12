#!/usr/bin/env python3
"""
Flik Live Log Generator
Generates realistic, continuous log streams for testing Flik's follow mode (`tail -F`),
format detection, real-time filtering, coloration, and field inspection.

Supported formats:
  - mixed        : Realistic mix of Nginx/Apache, Error logs, K8s, and JSON microservices
  - nginx        : Nginx Combined access logs (clean, realistic HTTP transactions)
  - nginx_access : Alias for nginx access logs
  - nginx_error  : Nginx upstream & server error logs (levels, pid#tid, upstream)
  - apache       : Apache Combined / Common access logs
  - apache_error : Apache error logs ([php:error], [core:notice], [http2:warn])
  - k8s          : Kubernetes container logs (stdout/stderr RFC3339)
  - json         : Structured NDJSON microservices logs (auth, payment, order)

Usage:
  python3 generate_live_logs.py live_test.log
  python3 generate_live_logs.py --format nginx --delay 0.1 live_test.log
  python3 generate_live_logs.py --format nginx_error live_test.log
  python3 generate_live_logs.py --format json --delay 0.1 live_test.log
  python3 generate_live_logs.py --format apache --burst live_test.log
  python3 generate_live_logs.py --stdout | ./flik.sh -
"""

import argparse
import json
import os
import random
import sys
import time
from datetime import datetime, timezone

# -----------------------------------------------------------------------------
# Data pools for realistic generation
# -----------------------------------------------------------------------------
IPS = [
    "192.168.1.10", "192.168.1.15", "192.168.1.42", "192.168.2.100",
    "10.0.4.25", "10.0.8.44", "10.0.12.80", "172.16.8.99", "172.16.14.12",
    "82.165.197.1", "162.158.138.45", "162.158.138.151", "127.0.0.1"
]

SERVICES = [
    "auth-service", "payment-gateway", "order-service",
    "inventory-api", "notification-worker", "cache-redis", "ingress-controller"
]

ERROR_MESSAGES = [
    "Connection to PostgreSQL database timed out after 5000ms",
    "Redis cache connection pool exhausted (max 100 connections)",
    "Payment authorization failed: Insufficient funds or card declined",
    "JWT token expired: signature verification failed",
    "Upstream service payment-gateway returned HTTP 504 Gateway Timeout",
    "Kafka broker unreachable on kafka.prod.svc.cluster.local:9092",
    "Disk I/O warning: write queue depth exceeded threshold (45ms)",
    "Memory threshold exceeded: cgroup limits reached (usage: 94.2%)",
]

PHP_SCRIPTS = [
    "/var/www/dada/this_is_a_new_hello_world.php",
    "/var/www/dada/ad_payement2.php",
    "/var/www/dada/get_my_tickets.php",
    "/var/www/dada/multibase.php",
    "/var/www/dada/read.php",
]

# -----------------------------------------------------------------------------
# Coherent HTTP Traffic Profiles
# -----------------------------------------------------------------------------
REQUEST_PROFILES = [
    # 1. Static Assets (Browsers requesting CSS, JS, images - 25% traffic)
    {
        "endpoints": ["/static/css/theme-dark.css", "/static/assets/bundle.min.js"],
        "methods": ["GET", "GET", "HEAD"],
        "users": ["-"],
        "statuses": [200, 200, 200, 304, 304, 404],
        "size_range": (1200, 38500),
        "user_agents": [
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Safari/605.1.15",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
            "Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/119.0",
        ],
        "referers": ["https://flik.io/app", "https://flik.io/dashboard"],
        "weight": 25,
    },
    # 2. Monitoring & Healthchecks (Prometheus / Datadog - 15% traffic)
    {
        "endpoints": ["/healthz", "/metrics"],
        "methods": ["GET"],
        "users": ["-"],
        "statuses": [200, 200, 200, 200, 503],
        "size_range": (64, 480),
        "user_agents": ["Prometheus/2.48.0", "Datadog-Agent/7.50.0", "curl/8.4.0"],
        "referers": ["-"],
        "weight": 15,
    },
    # 3. Search Engine Crawlers (Googlebot - 10% traffic)
    {
        "endpoints": ["/api/v1/products/search", "/api/v1/users/profile"],
        "methods": ["GET"],
        "users": ["-"],
        "statuses": [200, 200, 200, 404, 429],
        "size_range": (450, 6800),
        "user_agents": ["Googlebot/2.1 (+http://www.google.com/bot.html)"],
        "referers": ["-", "https://www.google.com/"],
        "weight": 10,
    },
    # 4. Authenticated Read APIs (Users reading catalog/profile - 25% traffic)
    {
        "endpoints": ["/api/v1/users/profile", "/api/v1/products/search", "/api/v1/cart/items"],
        "methods": ["GET"],
        "users": ["alice", "bob", "john", "-"],
        "statuses": [200, 200, 200, 304, 401, 403, 500],
        "size_range": (320, 11500),
        "user_agents": [
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Safari/605.1.15",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
            "PostmanRuntime/7.36.0",
        ],
        "referers": ["https://flik.io/app"],
        "weight": 25,
    },
    # 5. Authenticated Write APIs (Order checkout, Payment verification - 15% traffic)
    {
        "endpoints": ["/api/v1/orders/checkout", "/v1/payments/intent/verify"],
        "methods": ["POST"],
        "users": ["alice", "bob", "john"],
        "statuses": [200, 201, 201, 400, 402, 500, 502],
        "size_range": (180, 2400),
        "user_agents": [
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Safari/605.1.15",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
            "curl/8.4.0",
        ],
        "referers": ["https://flik.io/app"],
        "weight": 15,
    },
    # 6. Cart Mutations (PUT / DELETE - 5% traffic)
    {
        "endpoints": ["/api/v1/cart/items"],
        "methods": ["PUT", "DELETE"],
        "users": ["alice", "bob", "john"],
        "statuses": [200, 204, 400, 404, 500],
        "size_range": (0, 750),
        "user_agents": [
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Safari/605.1.15",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
        ],
        "referers": ["https://flik.io/app"],
        "weight": 5,
    },
    # 7. Token Refresh / Auth (5% traffic)
    {
        "endpoints": ["/api/v1/auth/token/refresh"],
        "methods": ["POST"],
        "users": ["-"],
        "statuses": [200, 200, 400, 401, 500],
        "size_range": (120, 620),
        "user_agents": [
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Safari/605.1.15",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
        ],
        "referers": ["https://flik.io/app"],
        "weight": 5,
    },
    # 8. Suspicious / Security probes (Admin scanner - 5% traffic)
    {
        "endpoints": ["/wp-login.php", "/admin/config/secrets"],
        "methods": ["GET", "POST"],
        "users": ["-", "admin"],
        "statuses": [401, 403, 404],
        "size_range": (150, 520),
        "user_agents": ["curl/8.4.0", "PostmanRuntime/7.36.0"],
        "referers": ["-"],
        "weight": 5,
    },
]

# -----------------------------------------------------------------------------
# Log Format Generators
# -----------------------------------------------------------------------------

def gen_http_access_log() -> str:
    """Generates standard combined access log lines (Nginx / Apache combined format)."""
    weights = [p["weight"] for p in REQUEST_PROFILES]
    profile = random.choices(REQUEST_PROFILES, weights=weights, k=1)[0]

    ip = random.choice(IPS)
    user = random.choice(profile["users"])
    now = datetime.now()
    ts = now.strftime("%d/%b/%Y:%H:%M:%S +0200")
    method = random.choice(profile["methods"])
    endpoint = random.choice(profile["endpoints"])
    status = random.choice(profile["statuses"])

    if status == 204:
        size = 0
    else:
        min_sz, max_sz = profile["size_range"]
        size = random.randint(min_sz, max_sz)

    referer = random.choice(profile["referers"])
    ua = random.choice(profile["user_agents"])

    return f'{ip} - {user} [{ts}] "{method} {endpoint} HTTP/1.1" {status} {size} "{referer}" "{ua}"'


def gen_apache_access() -> str:
    return gen_http_access_log()


def gen_nginx_access() -> str:
    return gen_http_access_log()


def gen_apache_error() -> str:
    now = datetime.now()
    ts = now.strftime("%a %b %d %H:%M:%S.%f %Y")
    pid = random.randint(10000, 99999)
    client = random.choice(IPS)
    client_port = random.randint(1024, 65535)

    rand = random.random()
    if rand < 0.60:
        # PHP Error
        script = random.choice(PHP_SCRIPTS)
        err_type = random.choice([
            "script not found or unable to stat",
            "Uncaught PDOException: SQLSTATE[HY000] [2002] Connection refused in multibase.php:48",
            "Call to undefined function get_user_session() in dada.php:124",
            "Maximum execution time of 30 seconds exceeded in get_my_tickets.php:89",
        ])
        return f'[{ts}] [php:error] [pid {pid}] [client {client}:{client_port}] script \'{script}\': {err_type}'
    elif rand < 0.85:
        # Notice / Info
        return f'[{ts}] [mpm_prefork:notice] [pid {pid}] AH00163: Apache/2.4.52 configured -- worker thread ready'
    else:
        # Warn
        return f'[{ts}] [core:warn] [pid {pid}] [client {client}:{client_port}] AH00560: Directory index forbidden by options directive: /var/www/html/uploads/'


def gen_nginx_error() -> str:
    """Generates standard Nginx error log lines with levels, pid#tid, connection, and metadata."""
    now = datetime.now()
    ts = now.strftime("%Y/%m/%d %H:%M:%S")
    pid = random.randint(1000, 8000)
    tid = random.randint(0, 9)
    conn = random.randint(1000, 99999)
    client = random.choice(IPS)
    server = random.choice(["api.flik.io", "cdn.flik.io", "auth.flik.io"])

    err_scenarios = [
        (
            f'connect() failed (111: Connection refused) while connecting to upstream, client: {client}, server: {server}, request: "POST /api/v1/auth/token/refresh HTTP/1.1", upstream: "http://127.0.0.1:8080/api/v1/auth/token/refresh", host: "{server}"',
            "error",
        ),
        (
            f'upstream timed out (110: Connection timed out) while reading response header from upstream, client: {client}, server: {server}, request: "GET /api/v1/users/profile HTTP/1.1", upstream: "http://10.0.8.44:3000/api/v1/users/profile", host: "{server}"',
            "warn",
        ),
        (
            f'client intended to send too large body: {random.randint(15, 80)}MB, client: {client}, server: {server}, request: "POST /v1/payments/intent/verify HTTP/1.1", host: "{server}"',
            "error",
        ),
        (
            f'open() "/var/www/static/css/theme-unknown.css" failed (2: No such file or directory), client: {client}, server: {server}, request: "GET /static/css/theme-unknown.css HTTP/1.1", host: "{server}"',
            "error",
        ),
        (
            f'limiting requests, excess: {random.randint(5, 25)}.000 by zone "api_limit", client: {client}, server: {server}, request: "GET /api/v1/products/search HTTP/1.1", host: "{server}"',
            "warn",
        ),
        (
            f'recv() failed (104: Connection reset by peer) while reading response header from upstream, client: {client}, server: {server}, request: "POST /api/v1/orders/checkout HTTP/1.1", upstream: "http://10.0.12.80:5000/api/v1/orders/checkout", host: "{server}"',
            "crit",
        ),
    ]

    reason, level = random.choice(err_scenarios)
    return f'{ts} [{level}] {pid}#{tid}: *{conn} {reason}'


def gen_k8s() -> str:
    now = datetime.now(timezone.utc)
    ts = now.strftime("%Y-%m-%dT%H:%M:%S.%f000Z")
    stream = "stderr" if random.random() < 0.25 else "stdout"
    flag = "F"

    # Container output can be plain text or JSON
    if random.random() < 0.50:
        svc = random.choice(SERVICES)
        status = random.choice([200, 200, 201, 400, 404, 500, 503])
        level = "info" if status < 400 else ("warn" if status < 500 else "error")
        payload = {
            "time": now.isoformat(),
            "level": level,
            "service": svc,
            "status": status,
            "msg": random.choice(ERROR_MESSAGES) if level == "error" else "Processed request successfully",
            "duration_ms": round(random.uniform(1.2, 450.0), 2),
        }
        msg = json.dumps(payload)
    else:
        if stream == "stderr":
            msg = f"[ERROR] {random.choice(ERROR_MESSAGES)}"
        else:
            msg = f"Started processing job batch_id={random.randint(1000, 9999)} partition={random.randint(0, 3)}"

    return f"{ts} {stream} {flag} {msg}"


def gen_json() -> str:
    now = datetime.now(timezone.utc)
    svc = random.choice(SERVICES)

    rand = random.random()
    if rand < 0.70:
        level = "info"
        status = random.choice([200, 201])
        msg = "Request handled successfully"
        err = None
    elif rand < 0.88:
        level = "warn"
        status = random.choice([400, 401, 403, 404, 429])
        msg = "Client warning: Rate limit or invalid parameter"
        err = "ERR_CLIENT_VALIDATION"
    else:
        level = "error"
        status = random.choice([500, 502, 503, 504])
        msg = random.choice(ERROR_MESSAGES)
        err = random.choice(["E_TIMEOUT", "E_DB_UNAVAILABLE", "E_AUTH_DOWN", "E_PAYMENT_REJECTED"])

    entry = {
        "time": now.isoformat(),
        "level": level,
        "service": svc,
        "status": status,
        "account_id": f"acc_{random.randint(100, 999)}",
        "transaction_id": f"txn_{random.randint(100000, 999999)}",
        "amount": round(random.uniform(5.0, 950.0), 2),
        "duration_ms": round(random.uniform(2.5, 380.0), 2),
        "msg": msg,
    }
    if err:
        entry["error"] = err

    return json.dumps(entry)


def gen_syslog_auth() -> str:
    now = datetime.now()
    ts = now.strftime("%b %d %H:%M:%S")
    host = "bastion-prod-01"
    pid = random.randint(1000, 65535)
    ip = random.choice(IPS)
    user = random.choice(["root", "admin", "ubuntu", "deploy", "guest", "alice", "bob", "scanner"])
    port = random.randint(1024, 65535)

    ev_type = random.random()
    if ev_type < 0.40:
        return f"{ts} {host} sshd[{pid}]: Failed password for invalid user {user} from {ip} port {port} ssh2"
    elif ev_type < 0.65:
        return f"{ts} {host} sshd[{pid}]: Accepted publickey for {user} from {ip} port {port} ssh2: RSA SHA256:{random.getrandbits(64):016x}"
    elif ev_type < 0.85:
        cmd = random.choice(["/bin/systemctl restart nginx", "/usr/bin/apt update", "/bin/cat /etc/shadow", "/bin/journalctl -u docker"])
        return f"{ts} {host} sudo:    {user} : TTY=pts/{random.randint(0, 4)} ; PWD=/home/{user} ; USER=root ; COMMAND={cmd}"
    else:
        return f"{ts} {host} sshd[{pid}]: pam_unix(sshd:auth): authentication failure; logname= uid=0 euid=0 tty=ssh ruser= rhost={ip}  user={user}"


def gen_logfmt() -> str:
    now = datetime.now(timezone.utc).isoformat()
    service = random.choice(SERVICES)
    level = random.choices(["info", "warn", "error"], weights=[0.7, 0.2, 0.1])[0]
    duration = f"{random.randint(5, 800)}ms"
    status = random.choices([200, 201, 400, 404, 500, 502], weights=[0.7, 0.1, 0.05, 0.05, 0.05, 0.05])[0]
    msg = random.choice(["request completed", "cache hit", "payment processed", "database query slow", "upstream timeout"])
    user_id = random.randint(100, 9999)
    return f'time="{now}" level={level} service={service} status={status} duration={duration} user_id={user_id} msg="{msg}"'


def gen_cloudwatch() -> str:
    now = datetime.now(timezone.utc)
    epoch_ms = int(now.timestamp() * 1000)
    stream_id = f"2026/11/05/[$LATEST]{random.getrandbits(32):08x}"
    trace_id = f"1-{random.getrandbits(32):08x}-{random.getrandbits(96):024x}"

    rand = random.random()
    if rand < 0.65:
        status = 200
        msg = f"REPORT RequestId: {random.getrandbits(64):016x} Duration: {random.uniform(5.0, 120.0):.2f} ms Billed Duration: 100 ms Memory Size: 512 MB Max Memory Used: 78 MB"
        level = "info"
    elif rand < 0.85:
        status = 404
        msg = f"WARN Request validation failed for resource /v1/items/{random.randint(100, 999)} - TraceId={trace_id}"
        level = "warn"
    else:
        status = random.choice([500, 502, 503, 504])
        err = random.choice(ERROR_MESSAGES)
        msg = f"ERROR Task timed out after 30.00 seconds: {err} - TraceId={trace_id}"
        level = "error"

    entry = {
        "@timestamp": now.strftime("%Y-%m-%dT%H:%M:%S.%f")[:-3] + "Z",
        "timestamp": epoch_ms,
        "@message": msg,
        "@logStream": stream_id,
        "logStreamName": stream_id,
        "status": status,
        "level": level,
        "trace_id": trace_id,
    }
    return json.dumps(entry)


def gen_gcp() -> str:
    now = datetime.now(timezone.utc)
    insert_id = f"gcp-{random.getrandbits(48):012x}"
    trace_id = f"00-{random.getrandbits(64):016x}{random.getrandbits(64):016x}-{random.getrandbits(64):016x}-01"

    rand = random.random()
    if rand < 0.65:
        severity = "INFO"
        status = 200
        msg = "GET /api/v2/inventory HTTP/1.1 200 OK"
    elif rand < 0.85:
        severity = "WARNING"
        status = random.choice([401, 403, 429])
        msg = f"Rate limit reached for client IP {random.choice(IPS)}"
    else:
        severity = random.choice(["ERROR", "CRITICAL"])
        status = random.choice([500, 503, 504])
        msg = random.choice(ERROR_MESSAGES)

    use_json_payload = random.random() < 0.6
    entry = {
        "insertId": insert_id,
        "timestamp": now.strftime("%Y-%m-%dT%H:%M:%S.%f")[:-3] + "Z",
        "severity": severity,
        "trace": f"projects/my-prod-gcp/traces/{trace_id}",
        "resource": {
            "type": "k8s_container",
            "labels": {
                "container_name": random.choice(SERVICES),
                "pod_name": f"app-worker-{random.getrandbits(16):04x}",
                "namespace_name": "production"
            }
        }
    }
    if use_json_payload:
        entry["jsonPayload"] = {
            "message": msg,
            "status": status,
            "client_ip": random.choice(IPS),
            "latency_ms": round(random.uniform(3.5, 420.0), 2)
        }
    else:
        entry["textPayload"] = f"[{severity}] {msg} (status: {status})"

    return json.dumps(entry)


def gen_mixed() -> str:
    choice = random.random()
    if choice < 0.20:
        return gen_http_access_log()
    elif choice < 0.35:
        return gen_cloudwatch()
    elif choice < 0.50:
        return gen_gcp()
    elif choice < 0.65:
        return gen_json()
    elif choice < 0.75:
        return gen_logfmt()
    elif choice < 0.85:
        return gen_syslog_auth()
    elif choice < 0.92:
        return gen_k8s()
    elif choice < 0.96:
        return gen_apache_error()
    else:
        return gen_nginx_error()


GENERATORS = {
    "mixed": gen_mixed,
    "cloudwatch": gen_cloudwatch,
    "aws": gen_cloudwatch,
    "gcp": gen_gcp,
    "stackdriver": gen_gcp,
    "nginx": gen_nginx_access,
    "nginx_access": gen_nginx_access,
    "nginx_error": gen_nginx_error,
    "apache": gen_apache_access,
    "apache_error": gen_apache_error,
    "k8s": gen_k8s,
    "json": gen_json,
    "logfmt": gen_logfmt,
    "auth": gen_syslog_auth,
    "syslog": gen_syslog_auth,
}

# -----------------------------------------------------------------------------
# Main Loop
# -----------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="Flik Live Log Generator - Continuous stream generator for tail -F testing"
    )
    parser.add_argument(
        "file",
        nargs="?",
        default="live_stream.log",
        help="Target log file (default: live_stream.log)",
    )
    parser.add_argument(
        "-f", "--format",
        choices=list(GENERATORS.keys()),
        default="mixed",
        help="Log format to generate (default: mixed)",
    )
    parser.add_argument(
        "-d", "--delay",
        type=float,
        default=0.2,
        help="Delay between log lines in seconds (default: 0.2 = ~5 lines/sec)",
    )
    parser.add_argument(
        "-b", "--burst",
        action="store_true",
        help="Randomly emit bursts of 10-30 logs at a time to test rapid indexing",
    )
    parser.add_argument(
        "-t", "--truncate",
        action="store_true",
        help="Truncate the log file at startup instead of appending",
    )
    parser.add_argument(
        "-n", "--count",
        type=int,
        default=None,
        help="Number of lines to generate before exiting (default: continuous stream)",
    )
    parser.add_argument(
        "--stdout",
        action="store_true",
        help="Output directly to stdout instead of a file",
    )

    args = parser.parse_args()

    generator_fn = GENERATORS[args.format]
    delay = args.delay
    if args.count is not None and "--delay" not in sys.argv and "-d" not in sys.argv:
        delay = 0.0

    # Mode: stdout or file
    is_stdout = args.stdout or (not sys.stdout.isatty() and args.file == "live_stream.log")
    if is_stdout:
        out_target = sys.stdout
    else:
        mode = "w" if (args.truncate or args.count) else "a"
        out_target = open(args.file, mode, encoding="utf-8")

    show_interactive = sys.stderr.isatty() and not is_stdout

    try:
        if show_interactive:
            sys.stderr.write("\033[1;36m" + "=" * 70 + "\033[0m\n")
            sys.stderr.write("\033[1;32m  ⚡ FLIK LIVE LOG GENERATOR ⚡\033[0m\n")
            sys.stderr.write(f"  Target File : \033[1;33m{os.path.abspath(args.file)}\033[0m\n")
            sys.stderr.write(f"  Format      : \033[1;35m{args.format}\033[0m\n")
            if args.count:
                sys.stderr.write(f"  Target Count: \033[1;32m{args.count:,} lines\033[0m\n")
            sys.stderr.write(f"  Speed       : \033[1;37m{delay:.2f}s per line\033[0m (burst: {args.burst})\n")
            sys.stderr.write("\033[1;36m" + "-" * 70 + "\033[0m\n")
            sys.stderr.write("  \033[1;33mPour tester dans Flik dans un autre terminal :\033[0m\n")
            sys.stderr.write(f"    \033[1;32m./flik.sh -f {args.file}\033[0m\n")
            sys.stderr.write("    (ou ouvrez Flik et appuyez sur \033[1;33m5\033[0m ou \033[1;33mt\033[0m pour activer le suivi en direct)\n")
            sys.stderr.write("\033[1;36m" + "=" * 70 + "\033[0m\n")
            sys.stderr.write("\033[0;90mGénération en cours... Appuyez sur Ctrl+C pour arrêter.\033[0m\n\n")
            sys.stderr.flush()

        line_count = 0
        start_time = time.time()

        while True:
            # Determine burst count
            burst_size = 1
            if args.burst and random.random() < 0.15:
                burst_size = random.randint(10, 25)

            for _ in range(burst_size):
                line = generator_fn()
                out_target.write(line + "\n")
                line_count += 1

                if show_interactive and (line_count % 50 == 0 or (args.count and line_count == args.count)):
                    elapsed = time.time() - start_time
                    lps = line_count / max(elapsed, 0.001)
                    preview = (line[:65] + "…") if len(line) > 65 else line
                    sys.stderr.write(
                        f"\r\033[K[\033[1;32m+{line_count}\033[0m lines | \033[1;36m{lps:.1f} logs/s\033[0m] {preview}"
                    )
                    sys.stderr.flush()

                if args.count and line_count >= args.count:
                    break

            if delay > 0:
                out_target.flush()
                time.sleep(delay)

            if args.count and line_count >= args.count:
                break

        if show_interactive and args.count:
            sys.stderr.write(f"\n\n\033[1;32m✓ Terminé : {line_count:,} lignes écrites avec succès dans {args.file}\033[0m\n")
            sys.stderr.flush()

    except (KeyboardInterrupt, BrokenPipeError):
        if show_interactive:
            sys.stderr.write("\n\n\033[1;33mArrêt de la génération de logs.\033[0m\n")
            sys.stderr.write(f"\033[1;32mTotal : {line_count} lignes écrites dans {args.file}\033[0m\n")
            sys.stderr.flush()
        else:
            try:
                sys.stdout.close()
            except Exception:
                pass
    finally:
        if not is_stdout:
            out_target.close()


if __name__ == "__main__":
    main()
