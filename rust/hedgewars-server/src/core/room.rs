use super::{
    client::HwClient,
    types::{
        ClientId, GameCfg, GameCfg::*, RoomConfig, RoomId, TeamInfo, Voting, MAX_HEDGEHOGS_PER_TEAM,
    },
};
use bitflags::*;
use serde::{Deserialize, Serialize};
use serde_derive::{Deserialize, Serialize};
use serde_yaml;
use std::{collections::HashMap, iter};

pub const MAX_TEAMS_IN_ROOM: u8 = 16;
pub const MAX_HEDGEHOGS_IN_ROOM: u8 = MAX_TEAMS_IN_ROOM * MAX_HEDGEHOGS_PER_TEAM;

fn client_teams_impl(
    teams: &[(ClientId, TeamInfo)],
    client_id: ClientId,
) -> impl Iterator<Item = &TeamInfo> + Clone {
    teams
        .iter()
        .filter(move |(id, _)| *id == client_id)
        .map(|(_, t)| t)
}

pub struct GameInfo {
    pub original_teams: Vec<(ClientId, TeamInfo)>,
    pub left_teams: Vec<String>,
    pub msg_log: Vec<String>,
    pub sync_msg: Option<String>,
    pub is_paused: bool,
    original_config: RoomConfig,
}

impl GameInfo {
    fn new(teams: Vec<(ClientId, TeamInfo)>, config: RoomConfig) -> GameInfo {
        GameInfo {
            left_teams: Vec::new(),
            msg_log: Vec::new(),
            sync_msg: None,
            is_paused: false,
            original_teams: teams,
            original_config: config,
        }
    }

    pub fn client_teams(&self, client_id: ClientId) -> impl Iterator<Item = &TeamInfo> + Clone {
        client_teams_impl(&self.original_teams, client_id)
    }

    pub fn mark_left_teams<'a, I>(&mut self, team_names: I)
    where
        I: Iterator<Item = &'a String>,
    {
        if let Some(m) = &self.sync_msg {
            self.msg_log.push(m.clone());
            self.sync_msg = None
        }

        for team_name in team_names {
            self.left_teams.push(team_name.clone());

            let remove_msg = crate::utils::to_engine_msg(iter::once(b'F').chain(team_name.bytes()));
            self.msg_log.push(remove_msg);
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct RoomSave {
    pub location: String,
    config: RoomConfig,
}

bitflags! {
    pub struct RoomFlags: u8 {
        const FIXED = 0b0000_0001;
        const RESTRICTED_JOIN = 0b0000_0010;
        const RESTRICTED_TEAM_ADD = 0b0000_0100;
        const REGISTRATION_REQUIRED = 0b0000_1000;
    }
}

pub struct HwRoom {
    pub id: RoomId,
    pub master_id: Option<ClientId>,
    pub name: String,
    pub password: Option<String>,
    pub greeting: String,
    pub protocol_number: u16,
    pub flags: RoomFlags,

    pub players_number: u8,
    pub default_hedgehog_number: u8,
    pub max_teams: u8,
    pub ready_players_number: u8,
    pub teams: Vec<(ClientId, TeamInfo)>,
    config: RoomConfig,
    pub voting: Option<Voting>,
    pub saves: HashMap<String, RoomSave>,
    pub game_info: Option<GameInfo>,
}

impl HwRoom {
    pub fn new(id: RoomId) -> HwRoom {
        HwRoom {
            id,
            master_id: None,
            name: String::new(),
            password: None,
            greeting: "".to_string(),
            flags: RoomFlags::empty(),
            protocol_number: 0,
            players_number: 0,
            default_hedgehog_number: 4,
            max_teams: MAX_TEAMS_IN_ROOM,
            ready_players_number: 0,
            teams: Vec::new(),
            config: RoomConfig::new(),
            voting: None,
            saves: HashMap::new(),
            game_info: None,
        }
    }

    pub fn hedgehogs_number(&self) -> u8 {
        self.teams.iter().map(|(_, t)| t.hedgehogs_number).sum()
    }

    pub fn addable_hedgehogs(&self) -> u8 {
        MAX_HEDGEHOGS_IN_ROOM - self.hedgehogs_number()
    }

    pub fn add_team(
        &mut self,
        owner_id: ClientId,
        mut team: TeamInfo,
        preserve_color: bool,
    ) -> &TeamInfo {
        if !preserve_color {
            team.color = iter::repeat(())
                .enumerate()
                .map(|(i, _)| i as u8)
                .take(u8::max_value() as usize + 1)
                .find(|i| self.teams.iter().all(|(_, t)| t.color != *i))
                .unwrap_or(0u8)
        };
        team.hedgehogs_number = if self.teams.is_empty() {
            self.default_hedgehog_number
        } else {
            self.teams[0]
                .1
                .hedgehogs_number
                .min(self.addable_hedgehogs())
        };
        self.teams.push((owner_id, team));
        &self.teams.last().unwrap().1
    }

    pub fn remove_team(&mut self, team_name: &str) {
        if let Some(index) = self.teams.iter().position(|(_, t)| t.name == team_name) {
            self.teams.remove(index);
        }
    }

    pub fn set_hedgehogs_number(&mut self, n: u8) -> Vec<String> {
        let mut names = Vec::new();
        let teams = &mut self.teams;

        if teams.len() as u8 * n <= MAX_HEDGEHOGS_IN_ROOM {
            for (_, team) in teams.iter_mut() {
                team.hedgehogs_number = n;
                names.push(team.name.clone())
            }
            self.default_hedgehog_number = n;
        }
        names
    }

    pub fn teams_in_game(&self) -> Option<u8> {
        self.game_info
            .as_ref()
            .map(|info| (info.original_teams.len() - info.left_teams.len()) as u8)
    }

    pub fn find_team_and_owner_mut<F>(&mut self, f: F) -> Option<(ClientId, &mut TeamInfo)>
    where
        F: Fn(&TeamInfo) -> bool,
    {
        self.teams
            .iter_mut()
            .find(|(_, t)| f(t))
            .map(|(id, t)| (*id, t))
    }

    pub fn find_team<F>(&self, f: F) -> Option<&TeamInfo>
    where
        F: Fn(&TeamInfo) -> bool,
    {
        self.teams
            .iter()
            .find_map(|(_, t)| Some(t).filter(|t| f(&t)))
    }

    pub fn client_teams(&self, client_id: ClientId) -> impl Iterator<Item = &TeamInfo> {
        client_teams_impl(&self.teams, client_id)
    }

    pub fn client_team_indices(&self, client_id: ClientId) -> Vec<u8> {
        self.teams
            .iter()
            .enumerate()
            .filter(move |(_, (id, _))| *id == client_id)
            .map(|(i, _)| i as u8)
            .collect()
    }

    pub fn clan_team_owners(&self, color: u8) -> impl Iterator<Item = ClientId> + '_ {
        self.teams
            .iter()
            .filter(move |(_, t)| t.color == color)
            .map(|(id, _)| *id)
    }

    pub fn clan_teams(&self, color: u8) -> impl Iterator<Item = &TeamInfo> {
        self.teams
            .iter()
            .filter(move |(_, t)| t.color == color)
            .map(|(_, team)| team)
    }

    pub fn clan_hedge_count(&self, color: u8) -> u8 {
        self.clan_teams(color).map(|t| t.hedgehogs_number).sum()
    }

    pub fn find_team_owner(&self, team_name: &str) -> Option<(ClientId, &str)> {
        self.teams
            .iter()
            .find(|(_, t)| t.name == team_name)
            .map(|(id, t)| (*id, &t.name[..]))
    }

    pub fn find_team_color(&self, owner_id: ClientId) -> Option<u8> {
        self.client_teams(owner_id).nth(0).map(|t| t.color)
    }

    pub fn has_multiple_clans(&self) -> bool {
        self.teams.iter().min_by_key(|(_, t)| t.color)
            != self.teams.iter().max_by_key(|(_, t)| t.color)
    }

    pub fn set_config(&mut self, cfg: GameCfg) {
        self.config.set_config(cfg);
    }

    pub fn start_round(&mut self) {
        if self.game_info.is_none() {
            self.game_info = Some(GameInfo::new(self.teams.clone(), self.config.clone()));
        }
    }

    pub fn is_fixed(&self) -> bool {
        self.flags.contains(RoomFlags::FIXED)
    }
    pub fn is_join_restricted(&self) -> bool {
        self.flags.contains(RoomFlags::RESTRICTED_JOIN)
    }
    pub fn is_team_add_restricted(&self) -> bool {
        self.flags.contains(RoomFlags::RESTRICTED_TEAM_ADD)
    }
    pub fn is_registration_required(&self) -> bool {
        self.flags.contains(RoomFlags::REGISTRATION_REQUIRED)
    }

    pub fn set_is_fixed(&mut self, value: bool) {
        self.flags.set(RoomFlags::FIXED, value)
    }
    pub fn set_join_restriction(&mut self, value: bool) {
        self.flags.set(RoomFlags::RESTRICTED_JOIN, value)
    }
    pub fn set_team_add_restriction(&mut self, value: bool) {
        self.flags.set(RoomFlags::RESTRICTED_TEAM_ADD, value)
    }
    pub fn set_unregistered_players_restriction(&mut self, value: bool) {
        self.flags.set(RoomFlags::REGISTRATION_REQUIRED, value)
    }

    fn flags_string(&self) -> String {
        let mut result = "-".to_string();
        if self.game_info.is_some() {
            result += "g"
        }
        if self.password.is_some() {
            result += "p"
        }
        if self.is_join_restricted() {
            result += "j"
        }
        if self.is_registration_required() {
            result += "r"
        }
        result
    }

    pub fn info(&self, master: Option<&HwClient>) -> Vec<String> {
        let c = &self.config;
        vec![
            self.flags_string(),
            self.name.clone(),
            self.players_number.to_string(),
            self.teams.len().to_string(),
            master.map_or("[]", |c| &c.nick).to_string(),
            c.map_type.to_string(),
            c.script.to_string(),
            c.scheme.name.to_string(),
            c.ammo.name.to_string(),
        ]
    }

    pub fn config(&self) -> &RoomConfig {
        &self.config
    }

    pub fn active_config(&self) -> &RoomConfig {
        match self.game_info {
            Some(ref info) => &info.original_config,
            None => &self.config,
        }
    }

    pub fn map_config(&self) -> Vec<String> {
        match self.game_info {
            Some(ref info) => info.original_config.to_map_config(),
            None => self.config.to_map_config(),
        }
    }

    pub fn game_config(&self) -> Vec<GameCfg> {
        match self.game_info {
            Some(ref info) => info.original_config.to_game_config(),
            None => self.config.to_game_config(),
        }
    }

    pub fn save_config(&mut self, name: String, location: String) {
        self.saves.insert(
            name,
            RoomSave {
                location,
                config: self.config.clone(),
            },
        );
    }

    pub fn load_config(&mut self, name: &str) -> Option<&str> {
        if let Some(save) = self.saves.get(name) {
            self.config = save.config.clone();
            Some(&save.location[..])
        } else {
            None
        }
    }

    pub fn delete_config(&mut self, name: &str) -> bool {
        self.saves.remove(name).is_some()
    }

    pub fn get_saves(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(&(&self.greeting, &self.saves))
    }

    pub fn set_saves(&mut self, text: &str) -> Result<(), serde_yaml::Error> {
        serde_yaml::from_str::<(String, HashMap<String, RoomSave>)>(text).map(
            |(greeting, saves)| {
                self.greeting = greeting;
                self.saves = saves;
            },
        )
    }
}
