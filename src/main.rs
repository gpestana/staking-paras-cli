#[subxt::subxt(
    runtime_metadata_path = "./artifacts/staing-parachain.scale",
    derive_for_type(path = "pallet_staking::ValidatorPrefs", derive = "Default"),
    derive_for_type(path = "sp_arithmetic::per_things::Perbill", derive = "Default"),
    derive_for_all_types = "Debug"
)]
pub mod staking_parachain {}

use structopt::StructOpt;

use subxt::{OnlineClient, SubstrateConfig};
use subxt_signer::sr25519::dev;

use crate::staking_parachain::runtime_types::{
    pallet_staking::RewardDestination, sp_core::sr25519::Signature,
};

use sp_core::{crypto::Pair, sr25519};
use sp_runtime::traits::Verify;

/// CLI for easy interaction with the staking-parachain.
#[derive(Debug, StructOpt)]
struct Opts {
    #[structopt(subcommand)]
    command: Command,
}

#[derive(Debug, StructOpt, Clone)]
enum Command {
    // Populate staking validators.
    #[structopt(name = "validate")]
    Validate {
        /// The id of the destination parachain.
        #[structopt(long, short, default_value = "2000")]
        parachain_id: u32,
        /// The number  of new validators.
        #[structopt(long, short, default_value = "10")]
        number: usize,
        /// RPC and signer configs.
        #[structopt(flatten)]
        configs: Configs,
    },
    #[structopt(name = "nominate")]
    Nominate {
        /// The id of the destination parachain.
        #[structopt(long, short, default_value = "2000")]
        parachain_id: u32,
        /// The number  of new validators.
        #[structopt(long, short, default_value = "10")]
        number: usize,
        /// The approx number of nominations per voter.
        #[structopt(long, default_value = "6")]
        nominations: usize,
        /// RPC and signer configs.
        #[structopt(flatten)]
        configs: Configs,
    },
}

/// Arguments required for creating and sending an extrinsic to a substrate node.
#[derive(Clone, Debug, StructOpt)]
pub(crate) struct Configs {
    /// RPC endpoint for the collator.
    #[structopt(name = "url", long, short)]
    url: String,
    /// Secret key for the ext signer.
    #[structopt(name = "skey", long, short)] // TODO: default is Alice
    skey: String,
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    env_logger::init();
    color_eyre::install()?;

    let _configs = match Opts::from_args().command {
        Command::Validate {
            parachain_id,
            number,
            configs,
        } => commands::validate(parachain_id, number, configs).await,
        Command::Nominate {
            parachain_id,
            number,
            nominations,
            configs,
        } => commands::nominate(parachain_id, number, nominations, configs).await,
    }?;

    // do somehting now with configs?

    Ok(())
}

mod commands {
    use super::*;

    use crate::staking_parachain::runtime_types::pallet_staking::ValidatorPrefs;

    /// Implements the validate command.
    pub(crate) async fn validate(
        _para_id: u32,
        number: usize,
        configs: Configs,
    ) -> color_eyre::Result<Configs> {
        let api = OnlineClient::<SubstrateConfig>::from_url(&configs.url).await?;

        let ed = staking_parachain::constants()
            .balances()
            .existential_deposit();
        let fund_with = api.constants().at(&ed)? * 10;

        let mut txs = vec![];

        // create accounts.
        for seed in 0..number {
            let pair = helpers::signer_from_seed(&seed.to_string());

            let mint_into_tx = staking_parachain::tx()
                .balances()
                .transfer_allow_death(pair.public_key().into(), fund_with);

            txs.push(mint_into_tx);

            //let bound_tx = staking_parachain::tx()
            //   .staking()
            //    .bond(balance, RewardDestination::Staked);
            //let validate_tx = staking_parachain::tx()
            //    .staking()
            //    .validate(Default::default());
        }

        println!("{:?}", txs);
        
        use subxt::ext::scale_value::{At, Composite, Value};

        let batch_tx = subxt::dynamic::tx(
            "Balances",
            "transfer_allow_death",
            vec![
            ],
        );

        let events = api
            .tx()
            .sign_and_submit_then_watch_default(&batch_tx, &dev::alice())
            .await?
            .wait_for_finalized_success()
            .await?;

        println!("{:?}", events);

        //let tx = staking_parachain::tx().utility().batch(txs);
        //let hash = api.tx().sign_and_submit_default(&tx, &dev::alice()).await?;

        Ok(configs)
    }

    /// Implements the nominate command.
    pub(crate) async fn nominate(
        _para_id: u32,
        _number: usize,
        _nominations: usize,
        configs: Configs,
    ) -> color_eyre::Result<Configs> {
        Ok(configs)
    }
}

mod helpers {
    use super::*;
    use std::io::Write;

    use subxt_signer::sr25519::Keypair;

    pub(crate) fn signer_from_seed(init_seed: &str) -> Keypair {
        let mut seed = [0; 32];
        let mut buffer = &mut seed[..];
        buffer.write(init_seed.as_bytes()).unwrap();

        Keypair::from_seed(seed).expect("generate keypair should be ok")
    }
}
