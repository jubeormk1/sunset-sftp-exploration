use argh::FromArgs;
#[allow(unused_imports)]
use log::{debug, error, info, log, trace, warn};

use embassy_executor::Spawner;
use embassy_net::{Stack, StackResources, StaticConfigV4};

use rand::rngs::OsRng;
use rand::RngCore;

use demo_common::menu::Runner as MenuRunner;
use embassy_futures::select::select;
use embassy_net_tuntap::TunTapDevice;
use embassy_sync::channel::Channel;
use embedded_io_async::Read;

use sunset::*;
use sunset_embassy::{ProgressHolder, SSHServer, SunsetMutex, SunsetRawMutex};

mod setupmenu;
pub(crate) use sunset_demo_embassy_common as demo_common;

use demo_common::{demo_menu, DemoServer, SSHConfig, ServerApp};

const NUM_LISTENERS: usize = 4;
// +1 for dhcp
const NUM_SOCKETS: usize = NUM_LISTENERS + 1;

const DEFAULT_TAP_DEVICE: &str = "tap99";
const DEFAULT_IP: &str = "192.168.69.2";

#[derive(FromArgs)]
/** #
 * Sunset SSH Server Embassy Demo on std.

This demo requires a tap device which the SSH Server will be attached on
*/
struct Args {
    #[argh(
        option,
        short = 'l',
        from_str_fn(parse_log_level),
        default = "log::LevelFilter::Info"
    )]
    /// logging filter: warn, info, debug, or trace (default "info")
    log_filter: log::LevelFilter,

    // #[argh(option, short = 'p', default = "2244")]
    // /// port
    // port: u16,
    #[argh(option, short = 'd', default = "DEFAULT_TAP_DEVICE.to_string()")]
    /// network device name (default: "tap99")
    device: String,

    #[argh(option, short = 'a', default = "DEFAULT_IP.to_string()")]
    /// IP address (default: "192.168.68.2")
    address: String,
    // #[argh(option)]
    // /// a path to hostkeys. At most one of each key type.
    // hostkey: Vec<String>,
}

fn parse_log_level(s: &str) -> Result<log::LevelFilter, String> {
    let log_level_filter = if s.eq_ignore_ascii_case("debug") {
        log::LevelFilter::Debug
    } else if s.eq_ignore_ascii_case("info") {
        log::LevelFilter::Info
    } else if s.eq_ignore_ascii_case("warn") {
        log::LevelFilter::Warn
    } else if s.eq_ignore_ascii_case("trace") {
        log::LevelFilter::Trace
    } else {
        return Err(
            "Please choose a log level from 'warn', 'info', 'debug', or 'trace'"
                .to_string(),
        );
    };
    Ok(log_level_filter)
}

fn parse_args_static() -> Result<&'static Args> {
    let args: Args = argh::from_env();
    let boxed_args = Box::new(args);
    Ok(Box::leak(boxed_args))
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, TunTapDevice>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn main_task(
    spawner: Spawner,
    ipv4: Option<&'static str>,
    tap_device: &'static str,
) {
    // TODO config

    // Leaking the heap content intentionally to avoid having to deal with lifetimes. config will live forever
    let config = Box::leak(Box::new({
        let mut config = SSHConfig::new().unwrap();

        config.set_admin_pw(Some("pw")).unwrap();
        config.console_noauth = true;

        config.ip4_static = if let Some(ipv4_address) = ipv4 {
            if let Ok(ip) = ipv4_address.parse() {
                Some(StaticConfigV4 {
                    address: embassy_net::Ipv4Cidr::new(ip, 24),
                    // gateway and dns server are not used in the server implementation
                    gateway: None,
                    dns_servers: { heapless::Vec::new() }, // no dns servers
                })
            } else {
                None
            }
        } else {
            None
        };

        SunsetMutex::new(config)
    }));

    let net_cf = if let Some(ref s) = config.lock().await.ip4_static {
        embassy_net::Config::ipv4_static(s.clone())
    } else {
        embassy_net::Config::dhcpv4(Default::default())
    };

    debug!("About to use tap device \"{}\"", tap_device);

    // Init network device
    let net_device = TunTapDevice::new(tap_device).unwrap();
    let seed = OsRng.next_u64();

    // Init network stack
    let res = Box::leak(Box::new(StackResources::<NUM_SOCKETS>::new()));
    let (stack, runner) = embassy_net::new(net_device, net_cf, res, seed);

    // Launch network task
    spawner.spawn(net_task(runner)).unwrap();

    for _ in 0..NUM_LISTENERS {
        spawner.spawn(listener(stack, config)).unwrap();
    }
}

#[derive(Default)]
struct StdDemo;

impl DemoServer for StdDemo {
    type Init = ();

    fn new(_init: &Self::Init) -> Self {
        Default::default()
    }

    async fn run(&self, serv: &SSHServer<'_>, mut common: ServerApp) -> Result<()> {
        let chan_pipe = Channel::<SunsetRawMutex, ChanHandle, 1>::new();

        let prog_loop = async {
            loop {
                let mut ph = ProgressHolder::new();
                let ev = serv.progress(&mut ph).await?;
                trace!("ev {ev:?}");
                match ev {
                    ServEvent::SessionShell(a) => {
                        if let Some(ch) = common.sess.take() {
                            debug_assert!(ch.num() == a.channel()?);
                            a.succeed()?;
                            let _ = chan_pipe.try_send(ch);
                        } else {
                            a.fail()?;
                        }
                    }
                    other => common.handle_event(other)?,
                };
            }
            #[allow(unreachable_code)]
            Ok::<_, Error>(())
        };

        let shell_loop = async {
            let ch = chan_pipe.receive().await;

            debug!("got handle");

            let mut stdio = serv.stdio(ch).await?;

            // input buffer, large enough for a ssh-ed25519 key
            let mut menu_buf = [0u8; 150];
            let menu_out = demo_menu::BufOutput::default();

            let mut menu = MenuRunner::new(
                &setupmenu::SETUP_MENU,
                &mut menu_buf,
                true,
                menu_out,
            );

            // bodge
            for c in "help\r\n".bytes() {
                menu.input_byte(c);
            }
            menu.context.flush(&mut stdio).await?;

            loop {
                let mut b = [0u8; 20];
                let lr = stdio.read(&mut b).await?;
                if lr == 0 {
                    break;
                }
                let b = &mut b[..lr];
                for c in b.iter() {
                    menu.input_byte(*c);
                }
                menu.context.flush(&mut stdio).await?;
            }
            Ok::<_, Error>(())
        };

        select(prog_loop, shell_loop).await;
        todo!()
    }
}

// TODO: pool_size should be NUM_LISTENERS but needs a literal
#[embassy_executor::task(pool_size = 4)]
async fn listener(
    stack: Stack<'static>,
    config: &'static SunsetMutex<SSHConfig>,
) -> ! {
    demo_common::listener::<StdDemo>(stack, config, ()).await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let args = parse_args_static().expect("Could not parse the arguments");

    env_logger::builder()
        .filter_level(args.log_filter)
        // .filter_module("sunset::runner", args.log)
        .filter_module("sunset::traffic", args.log_filter)
        .filter_module("sunset::encrypt", args.log_filter)
        // .filter_module("sunset::conn", args.log)
        // .filter_module("sunset_embassy::embassy_sunset", args.log)
        .filter_module("async_io", args.log_filter)
        .filter_module("polling", args.log_filter)
        .format_timestamp_nanos()
        .init();

    spawner
        .spawn(main_task(
            spawner,
            Some(&args.address.as_str()),
            args.device.as_str(),
        ))
        .unwrap();
}
