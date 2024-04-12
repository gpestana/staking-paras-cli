#[subxt::subxt(
    runtime_metadata_path = "./artifacts/staking-parachain.scale",
    derive_for_type(path = "pallet_staking::ValidatorPrefs", derive = "Default"),
    derive_for_type(path = "sp_arithmetic::per_things::Perbill", derive = "Default"),
    derive_for_all_types = "Debug"
)]
pub mod staking_parachain {}

use rand::prelude::*;
use structopt::StructOpt;

use subxt::{
    utils::{AccountId32, MultiAddress},
    OnlineClient, SubstrateConfig,
};
use subxt_signer::sr25519::dev;

use crate::staking_parachain::runtime_types::{
    pallet_balances::pallet::Call as BalancesCall, pallet_staking::RewardDestination,
    staking_rococo_runtime::RuntimeCall,
};

type Balance = u128; // fetch from Metadata
type ValidatorAccount = MultiAddress<AccountId32, ()>;

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
        #[structopt(long, default_value = "2000")]
        parachain_id: u32,
        /// The number  of new validators.
        #[structopt(long, default_value = "10")]
        number: usize,
        /// Balance to bond with
        #[structopt(long, default_value = "1000000000000")]
        bond_amount: Balance,
        /// RPC and signer configs.
        #[structopt(flatten)]
        configs: Configs,
    },
    #[structopt(name = "nominate")]
    Nominate {
        /// The id of the destination parachain.
        #[structopt(long, default_value = "2000")]
        parachain_id: u32,
        /// The number  of new validators.
        #[structopt(long, default_value = "10")]
        number: usize,
        /// Balance to bond with
        #[structopt(long, default_value = "1000000000000")]
        bond_amount: Balance,
        /// The approx number of nominations per voter.
        #[structopt(long, default_value = "6")]
        nominations: usize,
        /// RPC and signer configs.
        #[structopt(flatten)]
        configs: Configs,
    },
    #[structopt(name = "stakers_info")]
    StakersInfo {
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
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    env_logger::init();
    color_eyre::install()?;

    let _configs = match Opts::from_args().command {
        Command::Validate {
            parachain_id,
            number,
            bond_amount,
            configs,
        } => commands::validate(parachain_id, number, bond_amount, configs).await,
        Command::Nominate {
            parachain_id,
            number,
            bond_amount,
            nominations,
            configs,
        } => commands::nominate(parachain_id, number, bond_amount, nominations, configs).await,
        Command::StakersInfo { configs } => commands::stakers_info(configs).await,
    }?;

    Ok(())
}

mod commands {
    use super::*;

    /// Bonds and sets as validators `n_validators` new validators.
    pub(crate) async fn validate(
        _para_id: u32,
        n_validators: usize,
        bond_amount: Balance,
        configs: Configs,
    ) -> color_eyre::Result<Configs> {
        let api = OnlineClient::<SubstrateConfig>::from_url(&configs.url).await?;

        println!(
            "> Generating and funding, bonding and setting as validators {n_validators} accounts.."
        );

        let keypairs = helpers::fund_accounts(&api, n_validators, Some(bond_amount * 2)).await?;
        println!("Minting done for {n_validators} stakers.");

        let mut bond_calls: Vec<(_, _)> = vec![];
        let mut validate_calls: Vec<(_, _)> = vec![];

        // prepare both bond and validate calls for generated & funded keypairs.
        for pair in keypairs.into_iter() {
            let bond_tx = staking_parachain::tx()
                .staking()
                .bond(bond_amount, RewardDestination::Staked);
            let validate_tx = staking_parachain::tx()
                .staking()
                .validate(Default::default());

            bond_calls.push((pair.clone(), bond_tx));
            validate_calls.push((pair, validate_tx));
        }

        let mut it = bond_calls.into_iter().peekable();
        while let Some(next) = it.next() {
            let (pair, bond_tx) = next;
            let mut progress = api
                .tx()
                .sign_and_submit_then_watch_default(&bond_tx, &pair)
                .await?;
            // make sure all bonds went through before progressing.
            if it.peek().is_none() {
                while let Some(_) = progress.next().await {}
            }
        }
        println!("Bonding done for {n_validators} stakers.");

        let mut it = validate_calls.into_iter().peekable();
        while let Some(next) = it.next() {
            let (pair, validate_tx) = next;
            let mut progress = api
                .tx()
                .sign_and_submit_then_watch_default(&validate_tx, &pair)
                .await?;
            // make sure all bonds went through before progressing.
            if it.peek().is_none() {
                while let Some(_) = progress.next().await {}
            }
        }
        println!("Validating done for {n_validators} stakers.");

        Ok(configs)
    }

    /// Implements the nominate command.
    pub(crate) async fn nominate(
        _para_id: u32,
        n_nominators: usize,
        bond_amount: Balance,
        nominations: usize,
        configs: Configs,
    ) -> color_eyre::Result<Configs> {
        let api = OnlineClient::<SubstrateConfig>::from_url(&configs.url).await?;

        println!(
            "> Generating and funding, bonding and setting as nominators {n_nominators} accounts.."
        );

        let keypairs = helpers::fund_accounts(&api, n_nominators, Some(bond_amount * 2)).await?;
        println!("Minting done for {n_nominators} stakers.");

        let mut bond_calls: Vec<(_, _)> = vec![];
        let mut nominate_calls: Vec<(_, _)> = vec![];

        let current_validators = helpers::get_validators(&api).await?;

        // prepare both bond and nominate calls for generated & funded keypairs.
        for pair in keypairs.into_iter() {
            let targets = helpers::select_targets(nominations, &current_validators);

            let bond_tx = staking_parachain::tx()
                .staking()
                .bond(bond_amount, RewardDestination::Staked);
            let nominate_tx = staking_parachain::tx().staking().nominate(targets);

            bond_calls.push((pair.clone(), bond_tx));
            nominate_calls.push((pair, nominate_tx));
        }

        let mut it = bond_calls.into_iter().peekable();
        while let Some(next) = it.next() {
            let (pair, bond_tx) = next;
            let mut progress = api
                .tx()
                .sign_and_submit_then_watch_default(&bond_tx, &pair)
                .await?;
            // make sure all bonds went through before progressing.
            if it.peek().is_none() {
                while let Some(_) = progress.next().await {}
            }
        }
        println!("Bonding done for {n_nominators} stakers.");

        let mut it = nominate_calls.into_iter().peekable();
        while let Some(next) = it.next() {
            let (pair, nominate_tx) = next;

            println!("{:?}, {:?}\n", nominate_tx, pair);
            let mut progress = api
                .tx()
                .sign_and_submit_then_watch_default(&nominate_tx, &pair)
                .await?;
            // make sure all bonds went through before progressing.
            if it.peek().is_none() {
                while let Some(_) = progress.next().await {}
            }
        }
        println!("Nominations done for {n_nominators} stakers.");

        Ok(configs)
    }

    /// Fetches the current stakers info.
    pub(crate) async fn stakers_info(configs: Configs) -> color_eyre::Result<Configs> {
        let api = OnlineClient::<SubstrateConfig>::from_url(&configs.url).await?;

        let validators = helpers::get_validators(&api).await?;
        let nominators = helpers::get_nominators(&api).await?;

        println!("> Stakers info:");
        println!(" {:?} validators registered.", validators.len());
        println!(" {:?} nominators registered.", nominators.len());

        Ok(configs)
    }
}

mod helpers {
    use super::*;
    use std::io::Write;
    use subxt_signer::sr25519::Keypair;

    /// Randomly generates and funds `n` accounts. The vec of key paurs of the generated accounts
    /// are returned.
    pub(crate) async fn fund_accounts(
        api: &OnlineClient<SubstrateConfig>,
        n: usize,
        amount: Option<Balance>,
    ) -> color_eyre::Result<Vec<Keypair>> {
        let ed = staking_parachain::constants()
            .balances()
            .existential_deposit();
        let fund_with = amount.unwrap_or(api.constants().at(&ed)? * 1000);

        let mut pairs: Vec<Keypair> = vec![];
        let mut mint_calls: Vec<RuntimeCall> = vec![];

        // generate and fund new accounts:
        // - generate random keypair
        // - funds account
        for _n in 0..n {
            let mut rng = rand::thread_rng();
            let seed: usize = rng.gen();
            let pair = helpers::signer_from_seed(&seed.to_string());

            pairs.push(pair.clone());

            let mint_call = BalancesCall::transfer_allow_death {
                dest: pair.public_key().into(),
                value: fund_with,
            };
            mint_calls.push(RuntimeCall::Balances(mint_call));
        }

        let tx = staking_parachain::tx().utility().batch(mint_calls);
        let mut progress = api
            .tx()
            .sign_and_submit_then_watch_default(&tx, &dev::alice())
            .await?;
        // make sure all mints went through before progressing.
        while let Some(_) = progress.next().await {}

        Ok(pairs)
    }

    /// Fetches all validators registered in the system.
    pub(crate) async fn get_validators(
        api: &OnlineClient<SubstrateConfig>,
    ) -> color_eyre::Result<Vec<ValidatorAccount>> {
        let mut validators = vec![];
        let storage_query = staking_parachain::storage().staking().validators_iter();

        let mut results = api.storage().at_latest().await?.iter(storage_query).await?;
        while let Some(Ok(kv)) = results.next().await {
            let (k, _) = kv;
            let account: Vec<u8> = k.into_iter().rev().take(32).collect();
            let account: [u8; 32] = account.try_into().expect("32 bytes should fit");
            let maddress: MultiAddress<AccountId32, ()> = MultiAddress::Address32(account);
            validators.push(maddress);
        }

        Ok(validators)
    }

    /// Fetches all the nominators registered in the systen.
    pub(crate) async fn get_nominators(
        api: &OnlineClient<SubstrateConfig>,
    ) -> color_eyre::Result<Vec<ValidatorAccount>> {
        let mut nominators = vec![];
        let storage_query = staking_parachain::storage().staking().nominators_iter();

        let mut results = api.storage().at_latest().await?.iter(storage_query).await?;
        while let Some(Ok(kv)) = results.next().await {
            let (k, _) = kv;
            let account: Vec<u8> = k.into_iter().rev().take(32).collect();
            let account: [u8; 32] = account.try_into().expect("32 bytes should fit");
            let maddress: MultiAddress<AccountId32, ()> = MultiAddress::Address32(account);
            nominators.push(maddress);
        }

        Ok(nominators)
    }

    /// Selects a random `n` number of targets from a vec of validators.
    pub(crate) fn select_targets(
        n: usize,
        validators: &Vec<ValidatorAccount>,
    ) -> Vec<ValidatorAccount> {
        validators
            .choose_multiple(&mut rand::thread_rng(), n)
            .cloned()
            .collect::<Vec<_>>()
    }

    /// Generates a key pair from an init seed.
    pub(crate) fn signer_from_seed(init_seed: &str) -> Keypair {
        let mut seed = [0; 32];
        let mut buffer = &mut seed[..];
        buffer.write(init_seed.as_bytes()).unwrap();

        Keypair::from_seed(seed).expect("generate keypair should be ok")
    }
}
