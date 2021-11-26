use bevy::prelude::*;
use machinery::{AddSimulationExtension, Simulation, SimulationSteps};
use simulation::CoinSimResults;

fn main() {
    App::build()
        .add_plugins(DefaultPlugins)
        // Number of steps that each simulation will take before the main loop runs again
        .insert_resource(SimulationSteps(10))
        // Stores the data collected from all of our simulation worlds
        .init_resource::<Vec<CoinSimResults>>()
        // This "trait extension method" does several things:
        // 1. Adds each simulation in its own resource
        // 2. Adds a system to run each simulation world
        // 3. Adds a system to grab the data from the simulations and collect it
        .add_simulation(Simulation::<1>::new(0.5, 100))
        // We can add more copies of our simulation in their own worlds
        .add_simulation(Simulation::<2>::new(0.1, 100))
        // Modifying the parameters as we please
        .add_simulation(Simulation::<3>::new(1.0, 400))
        // Systems added to your app will operate on the main world
        // This system is added to PostUpdate, as the simulations themselves are set to run in Update
        .add_system_to_stage(CoreStage::PostUpdate, analysis::report_simulation.system())
        .run();
}

/// Code that is used to set up the multiple-worlds architecture
mod machinery {
    use super::simulation::CoinSimResults;
    use bevy::app::AppBuilder;
    use bevy::ecs::schedule::Schedule;
    use bevy::ecs::system::{IntoSystem, Res, ResMut};
    use bevy::ecs::world::World;

    // We can insert many copies of this, which can be operated on in parallel
    // As long as we choose a new value for N
    pub struct Simulation<const N: usize> {
        // Stores your data
        pub world: World,
        // Stores your systems
        pub schedule: Schedule,
    }

    pub struct SimulationSteps(pub isize);

    fn run_simulation<const N: usize>(
        mut simulation: ResMut<Simulation<N>>,
        steps: Res<SimulationSteps>,
    ) {
        // Bypass the borrow-checker being dumb about DerefMut
        let simulation = &mut *simulation;

        // Fetches the appropriate Simulation resource from the main world
        // Then runs the simulation schedule on the simulation world repeatedly
        for _ in 0..steps.0 {
            simulation.schedule.run_once(&mut simulation.world);
        }
    }

    fn collect_data<const N: usize>(
        mut collected_data: ResMut<Vec<CoinSimResults>>,
        simulation: Res<Simulation<N>>,
    ) {
        // Grab the data
        let sim_data = simulation.world.get_resource::<CoinSimResults>().unwrap();

        collected_data.push(sim_data.clone());
    }

    // Dummy trait, used to allow us to add a "trait extension method" to AppBuilder
    pub trait AddSimulationExtension {
        // `steps` controls the number of times the simulation will run
        // for each pass of the main analysis loop
        fn add_simulation<const N: usize>(&mut self, sim: Simulation<N>) -> &mut Self;
    }

    impl AddSimulationExtension for AppBuilder {
        fn add_simulation<const N: usize>(&mut self, sim: Simulation<N>) -> &mut Self {
            // Add the simulation as a resource in the main world
            self.insert_resource(sim)
                // Adds a system that runs our simulation `steps` number of times
                // to CoreStage::Update in the main world
                .add_system(run_simulation::<N>.system())
                // Collects the data from the simulation into the central storage
                .add_system(collect_data::<N>.system())
        }
    }
}

/// Code that is used to define how our individual simulations should work
// We're using a coin flipping simulation for demo purposes
mod simulation {
    use super::machinery::Simulation;
    use bevy::prelude::*;
    use rand::{
        distributions::Bernoulli, distributions::Distribution, rngs::SmallRng, SeedableRng,
    };

    /// The results of our simulation
    #[derive(Clone)]
    pub struct CoinSimResults {
        // You should store both the simulation parameters
        pub p: f64,
        // And the results
        pub n_tosses: isize,
        pub n_heads: isize,
    }

    /// Component that stores coin parameters
    struct CoinOdds {
        /// Probability of getting heads
        p: f64,
    }

    // Component that stores coin state
    #[derive(PartialEq, Eq)]
    enum CoinState {
        Heads,
        Tails,
    }

    // Resource that stores simulation parameters
    struct NTosses(isize);

    impl<const N: usize> Simulation<N> {
        // By using constructor methods, we can control the settings of our simulations
        pub fn new(p: f64, n_tosses: isize) -> Self {
            // Asserting that your parameters is within range
            // is just good practice
            assert!(p >= 0.0);
            assert!(p <= 1.0);

            // You can perform setup on the worlds here
            // Or you could add startup systems to your schedule
            let mut world = World::new();

            // Use spawn_batch for better performance
            for _ in 0..n_tosses {
                world.spawn().insert(CoinOdds { p });
            }

            // Storing configuration in resources
            world.insert_resource(NTosses(n_tosses));
            // Cheap source of seeded entropy
            world.insert_resource(SmallRng::seed_from_u64(42));

            // Storing data collection in a resource
            world.insert_resource(CoinSimResults {
                p,
                n_tosses: 0,
                n_heads: 0,
            });

            // Schedules contain stages contain systems
            let mut schedule = Schedule::default();
            // Each of our stages can be single-threaded,
            // since we're already parallelizing across our worlds
            let mut simulation_stage = SystemStage::single_threaded();
            simulation_stage.add_system(flip_coins.system());

            let mut recording_stage = SystemStage::single_threaded();
            recording_stage.add_system(record_coins.system());
            recording_stage.add_system(reset_coins.system());

            // You only need to add new stages when you need to process more commands
            schedule.add_stage("simulation", simulation_stage);
            schedule.add_stage("recording", recording_stage);

            // Return an instance of our Simulation type,
            // to be used as a resource in the main world
            Self { world, schedule }
        }
    }

    fn flip_coins(
        mut commands: Commands,
        query: Query<(Entity, &CoinOdds)>,
        mut rng: ResMut<SmallRng>,
    ) {
        for (entity, odds) in query.iter() {
            // Obviously generating random values one at a time like this
            // is pointlessly slow
            let distribution = Bernoulli::new(odds.p).unwrap();
            let was_heads = distribution.sample(&mut *rng);
            if was_heads {
                commands.entity(entity).insert(CoinState::Heads);
            } else {
                commands.entity(entity).insert(CoinState::Tails);
            }
        }
    }

    fn record_coins(query: Query<&CoinState>, mut coin_sim_results: ResMut<CoinSimResults>) {
        for coin_state in query.iter() {
            coin_sim_results.n_tosses += 1;
            if *coin_state == CoinState::Heads {
                coin_sim_results.n_heads += 1;
            }
        }
    }

    fn reset_coins(query: Query<Entity, With<CoinState>>, mut commands: Commands) {
        for entity in query.iter() {
            commands.entity(entity).remove::<CoinState>();
        }
    }
}

/// Code that analyses or relies on our simulation results
mod analysis {
    use crate::simulation::CoinSimResults;
    use bevy::prelude::*;

    pub fn report_simulation(results: Res<Vec<CoinSimResults>>) {
        for coin_sim_results in results.iter() {
            println!(
                "Coin trial with probability {} of heads",
                coin_sim_results.p
            );
            println!(
                "{} heads out of {} tosses",
                coin_sim_results.n_heads, coin_sim_results.n_tosses
            );
        }
    }
}
