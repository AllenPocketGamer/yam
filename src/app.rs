use super::misc::PulseTimer;
use legion::{
    systems::{Builder, ParallelRunnable, Runnable},
    Resources, Schedule, World,
};
use std::{
    cell::RefCell,
    fmt,
    panic,
    rc::Rc,
    slice::{Iter, IterMut},
};

#[derive(Default, Debug)]
pub struct App {
    busy_stages: Vec<AppStage>,
}

impl App {
    pub fn new() -> Self {
        Self {
            busy_stages: Default::default(),
        }
    }

    pub fn from_stages(stages: Vec<AppStage>) -> Self {
        Self { busy_stages: stages }
    }

    /// # Panics
    ///
    /// Panics if the ownership of `AppSettings` moved to outer
    pub fn run(self) {
        // take busy_stages out of the app and drop the app
        let busy_stages = Rc::new(RefCell::new(self.busy_stages));

        fn apply_and_ask_quit(resources: &mut Resources) -> bool {
            if resources.contains::<AppSettings>() {
                resources.get_mut::<AppSettings>().unwrap().apply()
            } else {
                panic!("dont move AppSettings out from Resources");
            }
        }

        let mut world = World::default();
        let mut resources = Resources::default();

        resources.insert::<AppSettings>(AppSettings::new(&busy_stages));

        for stage in RefCell::borrow(&busy_stages).iter() {
            stage.init(&mut world, &mut resources);
        }

        while !apply_and_ask_quit(&mut resources) {
            for stage in RefCell::borrow(&busy_stages).iter() {
                stage.play(&mut world, &mut resources);
            }
        }

        for stage in RefCell::borrow(&busy_stages).iter() {
            stage.free(&mut world, &mut resources);
        }
    }
}

#[derive(Default)]
pub struct AppBuilder {
    stage_builders: Vec<AppStageBuilder>,
}

impl AppBuilder {
    pub fn new() -> Self {
        Self {
            stage_builders: Default::default(),
        }
    }

    pub fn add_stage_builder(mut self, stage_builder: AppStageBuilder) -> Result<Self, AppBuildError> {
        if self.has_stage(stage_builder.name()) {
            Err(AppBuildError::DuplicateName(stage_builder))
        } else {
            self.stage_builders.push(stage_builder);
            Ok(self)
        }
    }

    pub fn create_stage_builder(self, stage_name: String, frequency: u32) -> Result<AppStageBuilder, AppBuildError> {
        let mut stage_builder = AppStageBuilder::new(stage_name, frequency);

        if self.has_stage(stage_builder.name()) {
            Err(AppBuildError::DuplicateName(stage_builder))
        } else {
            stage_builder.app_builder.replace(self);
            Ok(stage_builder)
        }
    }

    pub fn build(self) -> App {
        App::from_stages(self.stage_builders.into_iter().map(|stage_builder| stage_builder.build()).collect())
    }

    fn has_stage(&self, stage_name: &str) -> bool {
        self.stage_builders.iter().find(|stage| stage.name() == stage_name).is_some()
    }
}

#[derive(Debug)]
pub enum AppBuildError {
    DuplicateName(AppStageBuilder),
}

pub struct AppStage {
    name: String,
    timer: RefCell<PulseTimer>,

    startup: RefCell<Schedule>,
    process: RefCell<Schedule>,
    destroy: RefCell<Schedule>,
}

impl AppStage {
    fn new(name: String, timer: PulseTimer, startup: Schedule, process: Schedule, destroy: Schedule) -> Self {
        Self {
            name,
            timer: RefCell::new(timer),

            startup: RefCell::new(startup),
            process: RefCell::new(process),
            destroy: RefCell::new(destroy),
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn frequency(&self) -> u32 {
        self.timer.borrow().ticks_per_second()
    }

    pub fn set_frequency(&mut self, frequency: u32) {
        self.timer.borrow_mut().set_ticks_per_second(frequency);
    }

    pub(crate) fn init(&self, world: &mut World, resources: &mut Resources) {
        self.startup.borrow_mut().execute(world, resources);
    }

    pub(crate) fn play(&self, world: &mut World, resources: &mut Resources) {
        if self.timer.borrow_mut().update() {
            resources.insert::<PulseTimer>(*self.timer.borrow());

            self.process.borrow_mut().execute(world, resources);
        }
    }

    pub(crate) fn free(&self, world: &mut World, resources: &mut Resources) {
        self.destroy.borrow_mut().execute(world, resources);
    }
}

impl fmt::Debug for AppStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppStage")
            .field("name", &self.name)
            .field("frequency", &self.frequency())
            .finish()
    }
}

pub struct AppStageBuilder {
    name: String,
    frequency: u32,

    builder_startup: Builder,
    builder_process: Builder,
    builder_destroy: Builder,

    app_builder: Option<AppBuilder>,
}

impl fmt::Debug for AppStageBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppStageBuilder")
            .field("name", &self.name)
            .field("frequency", &self.frequency)
            .finish()
    }
}

impl AppStageBuilder {
    pub fn new(name: String, frequency: u32) -> Self {
        Self {
            name,
            frequency,

            builder_startup: Builder::default(),
            builder_process: Builder::default(),
            builder_destroy: Builder::default(),

            app_builder: None,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn frequency(&self) -> u32 {
        self.frequency
    }

    pub fn add_system_startup<T: ParallelRunnable + 'static>(mut self, system: T) -> Self {
        self.builder_startup.add_system(system);

        self
    }

    pub fn add_system_process<T: ParallelRunnable + 'static>(mut self, system: T) -> Self {
        self.builder_process.add_system(system);

        self
    }

    pub fn add_system_destroy<T: ParallelRunnable + 'static>(mut self, system: T) -> Self {
        self.builder_destroy.add_system(system);

        self
    }

    pub fn add_thread_local_system_startup<T: Runnable + 'static>(mut self, system: T) -> Self {
        self.builder_startup.add_thread_local(system);

        self
    }

    pub fn add_thread_local_system_process<T: Runnable + 'static>(mut self, system: T) -> Self {
        self.builder_process.add_thread_local(system);

        self
    }

    pub fn add_thread_local_system_destroy<T: Runnable + 'static>(mut self, system: T) -> Self {
        self.builder_destroy.add_thread_local(system);

        self
    }

    pub fn add_thread_local_fn_startup<F: FnMut(&mut World, &mut Resources) + 'static>(mut self, f: F) -> Self {
        self.builder_startup.add_thread_local_fn(f);

        self
    }

    pub fn add_thread_local_fn_process<F: FnMut(&mut World, &mut Resources) + 'static>(mut self, f: F) -> Self {
        self.builder_process.add_thread_local_fn(f);

        self
    }

    pub fn add_thread_local_fn_destroy<F: FnMut(&mut World, &mut Resources) + 'static>(mut self, f: F) -> Self {
        self.builder_destroy.add_thread_local_fn(f);

        self
    }

    pub fn build(mut self) -> AppStage {
        AppStage::new(
            self.name,
            PulseTimer::new(self.frequency),
            self.builder_startup.build(),
            self.builder_process.build(),
            self.builder_destroy.build(),
        )
    }

    pub fn into_app_builder(mut self) -> AppBuilder {
        let app_builder = if self.app_builder.is_some() {
            self.app_builder.take().unwrap()
        } else {
            AppBuilder::default()
        };

        app_builder.add_stage_builder(self).ok().unwrap()
    }
}

pub struct AppSettings {
    busy_stages: Rc<RefCell<Vec<AppStage>>>,

    spare_stages: Vec<AppStage>,
    commands: Vec<AppCommand>,
}

impl AppSettings {
    fn new(busy_stages: &Rc<RefCell<Vec<AppStage>>>) -> Self {
        Self {
            busy_stages: Rc::clone(busy_stages),

            spare_stages: Default::default(),
            commands: Default::default(),
        }
    }

    /// apply settings for app and return a flag indicating whether user request to quit
    fn apply(&mut self) -> bool {
        fn fuck_borrow_checker(busy_stages: &Vec<AppStage>, stage_name: &str) -> usize {
            busy_stages
                .iter()
                .enumerate()
                .find(|(_, stage)| stage.name() == stage_name)
                .map(|(index, _)| index)
                .unwrap()
        }

        for cmd in self.commands.drain(..) {
            match cmd {
                AppCommand::PushStageToWorkBefore { stage, after_stage_name } => {
                    let index = fuck_borrow_checker(&self.busy_stages.borrow(), after_stage_name.as_str());
                    self.busy_stages.borrow_mut().insert(index, stage);
                }
                AppCommand::PushStageToWork { stage } => {
                    self.busy_stages.borrow_mut().push(stage);
                }
                AppCommand::PushStageToWorkAfter { stage, before_stage_name } => {
                    let index = fuck_borrow_checker(&self.busy_stages.borrow(), before_stage_name.as_str());
                    self.busy_stages.borrow_mut().insert(index + 1, stage);
                }
                AppCommand::MakeBusyStageToRest { stage_name } => {
                    let index = fuck_borrow_checker(&self.busy_stages.borrow(), stage_name.as_str());
                    let stage = self.busy_stages.borrow_mut().remove(index);
                    self.spare_stages.push(stage);
                }
                AppCommand::SetBusyStageFrequency { stage_name, frequency } => {
                    self.busy_stages
                        .borrow_mut()
                        .iter_mut()
                        .find(|stage| stage.name() == stage_name)
                        .unwrap()
                        .set_frequency(frequency);
                }
                AppCommand::AppQuit => {
                    return true;
                }
            }
        }

        false
    }

    pub fn busy_stage<'a>(&'a self, stage_name: &str) -> Option<&'a AppStage> {
        let stages: &'a Vec<AppStage> = unsafe {
            // TODO: write safety words
            &self.busy_stages.try_borrow_unguarded().unwrap()
        };

        stages.iter().find(|stage| stage.name() == stage_name)
    }

    pub fn busy_stage_iter<'a>(&'a self) -> Iter<'a, AppStage> {
        let stages: &'a Vec<AppStage> = unsafe {
            // TODO: write safety words
            &self.busy_stages.try_borrow_unguarded().unwrap()
        };

        stages.iter()
    }

    pub fn spare_stage(&self, stage_name: &str) -> Option<&AppStage> {
        self.spare_stages.iter().find(|stage| stage.name() == stage_name)
    }

    pub fn spare_stage_iter(&self) -> Iter<AppStage> {
        self.spare_stages.iter()
    }

    pub fn spare_stage_mut(&mut self, stage_name: &str) -> Option<&mut AppStage> {
        self.spare_stages.iter_mut().find(|stage| stage.name() == stage_name)
    }

    pub fn spare_stage_iter_mut(&mut self) -> IterMut<AppStage> {
        self.spare_stages.iter_mut()
    }

    pub fn take_spare_stage(&mut self, stage_name: &str) -> Option<AppStage> {
        if let Some(index) = self
            .spare_stages
            .iter()
            .enumerate()
            .find(|(_, stage)| stage.name() == stage_name)
            .map(|(index, _)| index)
        {
            Some(self.spare_stages.remove(index))
        } else {
            None
        }
    }

    pub fn is_in_busy(&self, stage_name: &str) -> bool {
        self.busy_stages.borrow().iter().find(|stage| stage.name() == stage_name).is_some()
    }

    pub fn is_in_spare(&self, stage_name: &str) -> bool {
        self.spare_stages.iter().find(|stage| stage.name() == stage_name).is_some()
    }

    pub fn busy_stage_index<'a>(&self, stage_name: &'a str) -> Result<usize, AppSettingsError<'a>> {
        if let Some(index) = self
            .busy_stages
            .borrow()
            .iter()
            .enumerate()
            .find(|(_, stage)| stage.name() == stage_name)
            .map(|(index, _)| index)
        {
            Ok(index)
        } else {
            Err(AppSettingsError::StageNotExistInBusy(stage_name, None))
        }
    }

    pub fn push_stage_to_work_before<'a>(&mut self, stage: AppStage, after_stage_name: &'a str) -> Result<(), AppSettingsError<'a>> {
        if self.is_in_busy(after_stage_name) {
            if self.is_in_busy(stage.name()) {
                Err(AppSettingsError::DuplicateNameInBusy(stage))
            } else if self.is_in_spare(stage.name()) {
                Err(AppSettingsError::DuplicateNameInSpare(stage))
            } else {
                self.commands.push(AppCommand::PushStageToWorkBefore {
                    stage,
                    after_stage_name: String::from(after_stage_name),
                });

                Ok(())
            }
        } else {
            Err(AppSettingsError::StageNotExistInBusy(after_stage_name, Some(stage)))
        }
    }

    pub fn push_stage_to_work<'a>(&mut self, stage: AppStage) -> Result<(), AppSettingsError<'a>> {
        if self.is_in_busy(stage.name()) {
            Err(AppSettingsError::DuplicateNameInBusy(stage))
        } else if self.is_in_spare(stage.name()) {
            Err(AppSettingsError::DuplicateNameInSpare(stage))
        } else {
            self.commands.push(AppCommand::PushStageToWork { stage });

            Ok(())
        }
    }

    pub fn push_stage_to_work_after<'a>(&mut self, stage: AppStage, before_stage_name: &'a str) -> Result<(), AppSettingsError<'a>> {
        if self.is_in_busy(before_stage_name) {
            if self.is_in_busy(stage.name()) {
                Err(AppSettingsError::DuplicateNameInBusy(stage))
            } else if self.is_in_spare(stage.name()) {
                Err(AppSettingsError::DuplicateNameInSpare(stage))
            } else {
                self.commands.push(AppCommand::PushStageToWorkAfter {
                    stage,
                    before_stage_name: String::from(before_stage_name),
                });

                Ok(())
            }
        } else {
            Err(AppSettingsError::StageNotExistInBusy(before_stage_name, Some(stage)))
        }
    }

    pub fn make_spare_stage_work_before<'a>(&mut self, stage_name: &'a str, after_stage_name: &'a str) -> Result<(), AppSettingsError<'a>> {
        if let Some(stage) = self.take_spare_stage(stage_name) {
            self.push_stage_to_work_before(stage, after_stage_name)
        } else {
            Err(AppSettingsError::StageNotExistInSpare(stage_name, None))
        }
    }

    pub fn make_spare_stage_work<'a>(&mut self, stage_name: &'a str) -> Result<(), AppSettingsError<'a>> {
        if let Some(stage) = self.take_spare_stage(stage_name) {
            self.push_stage_to_work(stage)
        } else {
            Err(AppSettingsError::StageNotExistInSpare(stage_name, None))
        }
    }

    pub fn make_spare_stage_work_after<'a>(&mut self, stage_name: &'a str, before_stage_name: &'a str) -> Result<(), AppSettingsError<'a>> {
        if let Some(stage) = self.take_spare_stage(stage_name) {
            self.push_stage_to_work_after(stage, before_stage_name)
        } else {
            Err(AppSettingsError::StageNotExistInSpare(stage_name, None))
        }
    }

    pub fn push_stage_to_rest(&mut self, stage: AppStage) -> Result<(), AppSettingsError> {
        if self.is_in_busy(stage.name()) {
            Err(AppSettingsError::DuplicateNameInBusy(stage))
        } else if self.is_in_spare(stage.name()) {
            Err(AppSettingsError::DuplicateNameInSpare(stage))
        } else {
            self.spare_stages.push(stage);

            Ok(())
        }
    }

    pub fn make_busy_stage_rest<'a>(&mut self, stage_name: &'a str) -> Result<(), AppSettingsError<'a>> {
        if self.is_in_busy(stage_name) {
            self.commands.push(AppCommand::MakeBusyStageToRest {
                stage_name: String::from(stage_name),
            });

            Ok(())
        } else {
            Err(AppSettingsError::StageNotExistInBusy(stage_name, None))
        }
    }

    pub fn set_stage_frequency<'a>(&mut self, stage_name: &'a str, frequency: u32) -> Result<(), AppSettingsError<'a>> {
        if self.is_in_spare(stage_name) {
            self.spare_stage_mut(stage_name).unwrap().set_frequency(frequency);

            Ok(())
        } else if self.is_in_busy(stage_name) {
            // TODO: clear Commands that have same stage name
            self.commands.push(AppCommand::SetBusyStageFrequency {
                stage_name: String::from(stage_name),
                frequency,
            });

            Ok(())
        } else {
            Err(AppSettingsError::StageNotExist(stage_name))
        }
    }

    pub fn quit(&mut self) {
        self.commands.push(AppCommand::AppQuit);
    }
}

impl fmt::Debug for AppSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppSettings")
            .field("busy_stages", &RefCell::borrow(&self.busy_stages))
            .field("spare_stages", &self.spare_stages)
            .field("commands", &self.commands)
            .finish()
    }
}

#[derive(Debug)]
enum AppCommand {
    PushStageToWorkBefore { stage: AppStage, after_stage_name: String },
    PushStageToWork { stage: AppStage },
    PushStageToWorkAfter { stage: AppStage, before_stage_name: String },
    MakeBusyStageToRest { stage_name: String },
    SetBusyStageFrequency { stage_name: String, frequency: u32 },
    AppQuit,
}

#[derive(Debug)]
pub enum AppSettingsError<'a> {
    DuplicateNameInBusy(AppStage),
    DuplicateNameInSpare(AppStage),
    StageNotExist(&'a str),
    StageNotExistInBusy(&'a str, Option<AppStage>),
    StageNotExistInSpare(&'a str, Option<AppStage>),
}