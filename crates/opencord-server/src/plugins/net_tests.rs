//! Plugins' internet access (plan Task 10). The local site answers on
//! 127.0.0.1; the test resolver points public-looking names at it, which
//! only [`AddressPolicy::LoopbackForTests`] lets through.

use std::collections::{HashMap, HashSet, VecDeque};
use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, RwLock, Weak};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::HeaderValue;
use axum::http::header::CONNECTION;
use axum::response::{Redirect, Response};
use axum::routing::get;
use opencord_plugins::sandbox::{Vm, VmLimits};

use super::*;
use crate::plugins::api;
use crate::plugins::logbook::PluginLog;
use crate::plugins::permissions::PluginPermission;

const MIB: usize = 1024 * 1024;

fn ip(text: &str) -> IpAddr {
    text.parse().unwrap()
}

/// Answers each name with its next answer; the last one repeats.
#[derive(Default)]
struct Answers {
    by_host: Mutex<HashMap<String, VecDeque<Vec<IpAddr>>>>,
    asked: AtomicUsize,
}

impl Answers {
    fn new(entries: &[(&str, &[&[&str]])]) -> Arc<Answers> {
        let by_host = entries
            .iter()
            .map(|(host, answers)| {
                let answers = answers
                    .iter()
                    .map(|answer| answer.iter().map(|text| ip(text)).collect())
                    .collect();
                (host.to_string(), answers)
            })
            .collect();
        Arc::new(Answers {
            by_host: Mutex::new(by_host),
            asked: AtomicUsize::new(0),
        })
    }

    fn asked(&self) -> usize {
        self.asked.load(Ordering::SeqCst)
    }
}

impl Resolve for Answers {
    fn resolve(&self, host: &str) -> BoxFuture<'_, io::Result<Vec<IpAddr>>> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        let mut by_host = self.by_host.lock().unwrap();
        let answer = match by_host.get_mut(host) {
            Some(answers) if answers.len() > 1 => answers.pop_front(),
            Some(answers) => answers.front().cloned(),
            None => None,
        };
        Box::pin(async move { answer.ok_or_else(|| io::Error::other("no such name")) })
    }
}

fn guard(domains: &[&str]) -> Arc<NetGuard> {
    let domains: Vec<String> = domains.iter().map(|domain| domain.to_string()).collect();
    Arc::new(NetGuard::new(&domains))
}

fn net(policy: AddressPolicy, domains: &[&str], answers: Arc<Answers>) -> PluginNet {
    let guard = guard(domains);
    let client = build_client(guard.clone(), answers, policy).unwrap();
    PluginNet::with_client(guard, client)
}

/// A VM (for its budget) and the env its calls run with.
fn env(net: PluginNet, granted: &[PluginPermission]) -> (Vm, Arc<PluginEnv>) {
    let vm = Vm::new(&VmLimits {
        memory_bytes: 64 * MIB,
        handler_time: Duration::from_secs(5),
    })
    .unwrap();
    let env = Arc::new(PluginEnv {
        state: Weak::new(),
        plugin_id: "test".to_owned(),
        user_id: 7,
        version: "1.0.0".to_owned(),
        plugin_name: "Test".to_owned(),
        assets: Arc::new(HashSet::new()),
        granted: Arc::new(RwLock::new(granted.iter().copied().collect())),
        log: PluginLog::new("test", None),
        budget: vm.budget().clone(),
        server: tokio::runtime::Handle::current(),
        net,
        db: crate::plugins::database::PluginDb::unused(),
        files_dir: std::path::PathBuf::new(),
        files_bytes: 5120 * 1024 * 1024,
        hooks: Arc::new(RwLock::new(HashSet::new())),
        queue: Arc::new(crate::plugins::queue::EventQueue::default()),
        timers: Arc::new(Mutex::new(HashMap::new())),
        next_timer_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        config_values: Arc::new(RwLock::new(HashMap::new())),
        files_lock: Arc::new(tokio::sync::Mutex::new(())),
        files_reads: Arc::new(tokio::sync::Semaphore::new(10)),
        views_heard: Default::default(),
        sessions_heard: Default::default(),
        views_skip_logged: Default::default(),
    });
    (vm, env)
}

async fn get_url(env: &PluginEnv, url: &str) -> Result<HttpResponse, NetError> {
    let request = HttpRequest {
        method: Method::GET,
        url: Url::parse(url).unwrap(),
        headers: HeaderMap::new(),
        body: None,
        timeout: HTTP_TIMEOUT,
    };
    super::request(env, request).await
}

fn refused(host: &str) -> NetError {
    NetError::Network(format!(
        "{host} resolves only to addresses plugins may not reach"
    ))
}

/// Every answer closes its connection, so each request connects, and
/// looks its name up, anew.
async fn close(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(CONNECTION, HeaderValue::from_static("close"));
    response
}

fn redirect(to: String) -> axum::routing::MethodRouter {
    get(move || {
        let to = to.clone();
        async move { Redirect::to(&to) }
    })
}

/// A site on 127.0.0.1; returns its port.
async fn site() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = Router::new()
        .route("/hello", get(|| async { "hello" }))
        .route(
            "/big",
            get(|| async {
                // 10 MiB and one byte, without a length up front.
                let pieces = (0..10)
                    .map(|_| Bytes::from(vec![b'x'; MIB]))
                    .chain([Bytes::from_static(b"x")])
                    .map(Ok::<_, Infallible>);
                Body::from_stream(futures_util::stream::iter(pieces))
            }),
        )
        .route("/exact", get(|| async { vec![b'x'; 10 * MIB] }))
        .route(
            "/to-hello",
            redirect(format!("http://api.example.com:{port}/hello")),
        )
        .route(
            "/to-private",
            redirect(format!("http://private.example.com:{port}/hello")),
        )
        .route(
            "/to-literal",
            redirect(format!("http://127.0.0.1:{port}/hello")),
        )
        .route(
            "/to-other",
            redirect(format!("http://other.example.org:{port}/hello")),
        )
        .layer(axum::middleware::map_response(close));
    tokio::spawn(async move { axum::serve(listener, app).await });
    port
}

/// Runs `source` within a budget; it returns a string.
async fn run(vm: &Vm, source: &str) -> String {
    vm.budget().start(Duration::from_secs(30));
    let result = vm
        .load("test", source)
        .unwrap()
        .call_async::<String>(())
        .await;
    vm.budget().stop();
    result.unwrap()
}

fn actions(env: &PluginEnv) -> Vec<String> {
    env.log
        .lines(None, 100)
        .into_iter()
        .filter(|line| line.level == proto::PluginLogLevel::Action as i32)
        .map(|line| line.text)
        .collect()
}

#[test]
fn forbidden_ranges() {
    // Catches: a missing or mistyped range, and ranges cut too wide.
    let forbidden = [
        "0.0.0.0",
        "0.255.255.255",
        "10.0.0.0",
        "10.255.255.255",
        "100.64.0.0",
        "100.127.255.255",
        "127.0.0.1",
        "127.255.255.255",
        "169.254.0.0",
        "169.254.169.254",
        "172.16.0.0",
        "172.31.255.255",
        "192.0.0.0",
        "192.0.0.255",
        "192.0.2.0",
        "192.0.2.255",
        "192.88.99.0",
        "192.88.99.255",
        "192.168.0.0",
        "192.168.255.255",
        "198.18.0.0",
        "198.19.255.255",
        "198.51.100.0",
        "198.51.100.255",
        "203.0.113.0",
        "203.0.113.255",
        "224.0.0.0",
        "239.255.255.255",
        "240.0.0.0",
        "255.255.255.255",
        "::",
        "::1",
        "::ffff:10.0.0.1",
        "::ffff:169.254.169.254",
        "64:ff9b::a00:1",
        "64:ff9b::c0a8:101",
        "2002:a00:1::",
        "2002:c0a8:101::1",
        "100::",
        "100::ffff:ffff:ffff:ffff",
        "2001::",
        "2001:0:ffff:ffff:ffff:ffff:ffff:ffff",
        "2001:db8::",
        "2001:db8:ffff:ffff:ffff:ffff:ffff:ffff",
        "fc00::",
        "fdff:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
        "fe80::",
        "febf:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
        "fec0::",
        "feff:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
        "ff00::",
        "ff02::1",
        // Past the plan's list: ::a.b.c.d, and NAT64 for local use.
        "::7f00:1",
        "::a00:1",
        "64:ff9b:1::808:808",
    ];
    let allowed = [
        "1.0.0.0",
        "9.255.255.255",
        "11.0.0.0",
        "100.63.255.255",
        "100.128.0.0",
        "126.255.255.255",
        "128.0.0.0",
        "169.253.255.255",
        "169.255.0.0",
        "172.15.255.255",
        "172.32.0.0",
        "192.0.1.0",
        "192.0.3.0",
        "192.88.98.255",
        "192.88.100.0",
        "192.167.255.255",
        "192.169.0.0",
        "198.17.255.255",
        "198.20.0.0",
        "198.51.99.255",
        "198.51.101.0",
        "203.0.112.255",
        "203.0.114.0",
        "223.255.255.255",
        "8.8.8.8",
        "0:0:0:0:0:1::",
        "::ffff:8.8.8.8",
        "64:ff9b::808:808",
        "64:ff9a:ffff:ffff:ffff:ffff:ffff:ffff",
        "2002:808:808::",
        "2003::",
        "100:0:0:1::",
        "2001:1::",
        "2001:db7:ffff:ffff:ffff:ffff:ffff:ffff",
        "2001:db9::",
        "fbff:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
        "fe7f:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
        "2606:4700:4700::1111",
    ];
    for text in forbidden {
        assert!(is_forbidden(ip(text)), "{text} must be forbidden");
    }
    for text in allowed {
        assert!(!is_forbidden(ip(text)), "{text} must be allowed");
    }
}

#[tokio::test]
async fn ipv4_mapped_loopback_is_refused() {
    // Catches: judging an IPv6 address by IPv6 ranges alone, which lets
    // ::ffff:127.0.0.1 through to this machine.
    for text in ["::ffff:127.0.0.1", "64:ff9b::7f00:1", "2002:7f00:1::"] {
        assert!(is_forbidden(ip(text)), "{text} must be forbidden");
    }
    let port = site().await;
    let answers = Answers::new(&[("api.example.com", &[&["::ffff:127.0.0.1"]])]);
    let (_vm, env) = env(
        net(AddressPolicy::Public, &["api.example.com"], answers),
        &[],
    );
    let error = get_url(&env, &format!("http://api.example.com:{port}/hello"))
        .await
        .err();
    assert_eq!(error, Some(refused("api.example.com")));
}

#[tokio::test]
async fn a_name_resolving_to_a_private_address_is_refused() {
    // Catches: trusting the domains list and connecting wherever its names
    // point. The site really is at 127.0.0.1.
    let port = site().await;
    let answers = Answers::new(&[
        ("loop.example.com", &[&["127.0.0.1"]]),
        ("lan.example.com", &[&["10.1.2.3"]]),
        ("metadata.example.com", &[&["169.254.169.254"]]),
        ("six.example.com", &[&["::1"]]),
    ]);
    let (_vm, env) = env(net(AddressPolicy::Public, &["*.example.com"], answers), &[]);
    for host in [
        "loop.example.com",
        "lan.example.com",
        "metadata.example.com",
        "six.example.com",
    ] {
        let error = get_url(&env, &format!("http://{host}:{port}/hello"))
            .await
            .err();
        assert_eq!(error, Some(refused(host)));
    }
}

#[tokio::test]
async fn only_the_public_addresses_of_a_mixed_answer_are_used() {
    // Catches: refusing only answers that are all private, and connecting
    // to the private ones of a mixed answer.
    let answers = Answers::new(&[(
        "api.example.com",
        &[&[
            "10.0.0.1",
            "8.8.8.8",
            "::1",
            "2606:4700:4700::1111",
            "169.254.169.254",
        ]],
    )]);
    let guarded = Guarded {
        guard: guard(&["api.example.com"]),
        resolver: answers,
        policy: AddressPolicy::Public,
    };
    let found: Vec<IpAddr> =
        reqwest::dns::Resolve::resolve(&guarded, "api.example.com".parse().unwrap())
            .await
            .unwrap()
            .map(|address| address.ip())
            .collect();
    assert_eq!(found, [ip("8.8.8.8"), ip("2606:4700:4700::1111")]);

    let port = site().await;
    let answers = Answers::new(&[("api.example.com", &[&["192.168.0.1", "127.0.0.1"]])]);
    let (_vm, env) = env(
        net(
            AddressPolicy::LoopbackForTests,
            &["api.example.com"],
            answers,
        ),
        &[],
    );
    let response = get_url(&env, &format!("http://api.example.com:{port}/hello"))
        .await
        .unwrap();
    assert_eq!(response.body, b"hello");
}

#[tokio::test]
async fn each_connection_checks_the_address_again() {
    // Catches: checking once and caching.
    let port = site().await;
    let answers = Answers::new(&[("api.example.com", &[&["127.0.0.1"], &["10.0.0.1"]])]);
    let (_vm, env) = env(
        net(
            AddressPolicy::LoopbackForTests,
            &["api.example.com"],
            answers.clone(),
        ),
        &[],
    );
    let url = format!("http://api.example.com:{port}/hello");

    let first = get_url(&env, &url).await.unwrap();
    assert_eq!(first.body, b"hello");
    let second = get_url(&env, &url).await.err();
    assert_eq!(second, Some(refused("api.example.com")));
    assert_eq!(answers.asked(), 2);
}

#[tokio::test]
async fn a_redirect_to_a_private_address_is_refused() {
    // Catches: checking the first address only. The literal one would
    // reach the site.
    let port = site().await;
    let answers = Answers::new(&[
        ("api.example.com", &[&["127.0.0.1"]]),
        ("private.example.com", &[&["10.0.0.1"]]),
    ]);
    let (_vm, env) = env(
        net(
            AddressPolicy::LoopbackForTests,
            &["api.example.com", "private.example.com"],
            answers,
        ),
        &[],
    );

    let followed = get_url(&env, &format!("http://api.example.com:{port}/to-hello"))
        .await
        .unwrap();
    assert_eq!(followed.body, b"hello");
    let to_private = get_url(&env, &format!("http://api.example.com:{port}/to-private"))
        .await
        .err();
    assert_eq!(to_private, Some(refused("private.example.com")));
    let to_literal = get_url(&env, &format!("http://api.example.com:{port}/to-literal"))
        .await
        .err();
    assert_eq!(
        to_literal,
        Some(NetError::NotInDomains("127.0.0.1".to_owned()))
    );
}

#[tokio::test]
async fn a_redirect_to_another_domain_is_refused() {
    // Catches: following redirects with reqwest's own policy, which goes
    // anywhere. other.example.org would answer.
    let port = site().await;
    let answers = Answers::new(&[
        ("api.example.com", &[&["127.0.0.1"]]),
        ("other.example.org", &[&["127.0.0.1"]]),
    ]);
    let (_vm, env) = env(
        net(
            AddressPolicy::LoopbackForTests,
            &["api.example.com"],
            answers,
        ),
        &[],
    );
    let error = get_url(&env, &format!("http://api.example.com:{port}/to-other"))
        .await
        .err();
    assert_eq!(
        error,
        Some(NetError::NotInDomains("other.example.org".to_owned()))
    );
}

#[tokio::test]
async fn ip_literal_urls_are_refused() {
    // Catches: relying on the resolver, which the connector skips for an
    // address. Each of these would reach the site.
    let port = site().await;
    let answers = Answers::new(&[]);
    let (_vm, env) = env(
        net(
            AddressPolicy::LoopbackForTests,
            &["api.example.com"],
            answers.clone(),
        ),
        &[],
    );
    for (url, host) in [
        (format!("http://127.0.0.1:{port}/hello"), "127.0.0.1"),
        (format!("http://[::1]:{port}/hello"), "[::1]"),
        (format!("http://2130706433:{port}/hello"), "127.0.0.1"),
        (format!("http://0x7f.1:{port}/hello"), "127.0.0.1"),
        (
            format!("http://[::ffff:127.0.0.1]:{port}/hello"),
            "[::ffff:7f00:1]",
        ),
    ] {
        let error = get_url(&env, &url).await.err();
        assert_eq!(
            error,
            Some(NetError::NotInDomains(host.to_owned())),
            "{url}"
        );
    }
    assert_eq!(answers.asked(), 0);
}

#[tokio::test]
async fn a_large_response_is_cut() {
    // Catches: reading the whole body before measuring it, or trusting a
    // length the site didn't send.
    let port = site().await;
    let answers = Answers::new(&[("api.example.com", &[&["127.0.0.1"]])]);
    let (_vm, env) = env(
        net(
            AddressPolicy::LoopbackForTests,
            &["api.example.com"],
            answers,
        ),
        &[],
    );
    let big = get_url(&env, &format!("http://api.example.com:{port}/big"))
        .await
        .err();
    assert_eq!(
        big,
        Some(NetError::Network(
            "the response is larger than 10 MB".to_owned()
        ))
    );
    let exact = get_url(&env, &format!("http://api.example.com:{port}/exact"))
        .await
        .unwrap();
    assert_eq!(exact.body.len(), 10 * MIB);
}

#[tokio::test]
async fn requests_need_the_internet_permission() {
    // Catches: http.request checking the domains but not the permission.
    let port = site().await;
    let answers = Answers::new(&[("api.example.com", &[&["127.0.0.1"]])]);
    let source = format!(
        r#"
        local ok, answer = pcall(http.request, {{ url = "http://api.example.com:{port}/hello?key=secret" }})
        if not ok then
            return answer
        end
        return answer.status .. " " .. answer.body .. " " .. answer.headers["connection"]
        "#
    );

    let (vm, without) = env(
        net(
            AddressPolicy::LoopbackForTests,
            &["api.example.com"],
            answers.clone(),
        ),
        &[],
    );
    api::http::install(&vm.lua, &without).unwrap();
    assert_eq!(
        run(&vm, &source).await,
        "PERMISSION_DENIED: the plugin doesn't have internet"
    );
    assert_eq!(answers.asked(), 0);
    assert!(actions(&without).is_empty());

    let (vm, with) = env(
        net(
            AddressPolicy::LoopbackForTests,
            &["api.example.com"],
            answers,
        ),
        &[PluginPermission::Internet],
    );
    api::http::install(&vm.lua, &with).unwrap();
    assert_eq!(run(&vm, &source).await, "200 hello close");
    assert_eq!(
        actions(&with),
        [format!("GET api.example.com:{port}/hello → 200")]
    );
    let outside = r#"
        local ok, answer = pcall(http.request, { url = "https://example.org/" })
        return answer
    "#;
    assert_eq!(
        run(&vm, outside).await,
        "FORBIDDEN: example.org isn't in the plugin's domains"
    );
}

#[tokio::test]
async fn the_arguments_are_checked() {
    let answers = Answers::new(&[]);
    let (vm, env) = env(
        net(AddressPolicy::Public, &["api.example.com"], answers),
        &[PluginPermission::Internet],
    );
    api::http::install(&vm.lua, &env).unwrap();
    for (options, error) in [
        (
            "'https://api.example.com/'",
            "INVALID_ARGUMENT: http.request takes a table such as { url = \"https://…\" }",
        ),
        ("{}", "INVALID_ARGUMENT: url must be a string"),
        (
            "{ url = 'not an address' }",
            "INVALID_ARGUMENT: url isn't a valid address: relative URL without a base",
        ),
        (
            "{ url = 'ftp://api.example.com/' }",
            "NETWORK: only http and https addresses can be reached, not ftp",
        ),
        (
            "{ url = 'https://api.example.com/', method = 'CONNECT' }",
            "INVALID_ARGUMENT: method must be one of GET, HEAD, POST, PUT, PATCH, DELETE, OPTIONS",
        ),
        (
            "{ url = 'https://api.example.com/', headers = { Host = 'x' } }",
            "INVALID_ARGUMENT: the plugin may not set the host header",
        ),
        (
            "{ url = 'https://api.example.com/', headers = { ['Proxy-Authorization'] = 'x' } }",
            "INVALID_ARGUMENT: the plugin may not set the proxy-authorization header",
        ),
        (
            "{ url = 'https://api.example.com/', headers = { ['Transfer-Encoding'] = 'chunked' } }",
            "INVALID_ARGUMENT: the plugin may not set the transfer-encoding header",
        ),
        (
            "{ url = 'https://api.example.com/', headers = { accept = 5 } }",
            "INVALID_ARGUMENT: the accept header must be a string",
        ),
        (
            "{ url = 'https://api.example.com/', body = string.rep('x', 10 * 1024 * 1024 + 1) }",
            "INVALID_ARGUMENT: the body is larger than 10 MB",
        ),
        (
            "{ url = 'https://api.example.com/', timeout = 11 }",
            "INVALID_ARGUMENT: timeout must be more than 0 and at most 10 seconds",
        ),
        (
            "{ url = 'https://api.example.com/', timeout = 0 }",
            "INVALID_ARGUMENT: timeout must be more than 0 and at most 10 seconds",
        ),
    ] {
        let source =
            format!("local ok, answer = pcall(http.request, {options}) return tostring(answer)");
        assert_eq!(run(&vm, &source).await, error, "{options}");
    }
}
