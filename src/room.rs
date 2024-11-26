use std::{collections::{HashMap, HashSet}, io};

use chrono::Utc;
use rand::{thread_rng, Rng as _};
use tokio::sync::{mpsc, oneshot};


pub type RoomId = u32;
pub type ConnId = u32;
pub type Msg = String;

pub const USER_HOST : ConnId = 0;
pub const USER_CLIENT : ConnId = 1;

#[derive(Debug)]
pub struct RoomCreds{
    pub id: RoomId,
    pub token: String,
}

impl RoomCreds{
    pub fn new(id: RoomId, token: String) -> Self {
        Self{
            id,
            token,
        }
    }
}

/// A command received by the [`ChatServer`].
#[derive(Debug)]
enum Command {
    Create{
        host: String,
        res_tx: tokio::sync::oneshot::Sender<RoomCreds>,
    },

    RoomExists{
        room_id: RoomId,
        res_tx: tokio::sync::oneshot::Sender<bool>,
    },

    RoomHostAuth{
        room_id: RoomId,
        host_token: String,
        res_tx: tokio::sync::oneshot::Sender<bool>,
    },

    Connect {
        room: RoomId,
        conn_tx: mpsc::UnboundedSender<Msg>,
        res_tx: tokio::sync::oneshot::Sender<ConnId>,
        user_type: ConnId,
    },

    Disconnect {
        room: RoomId,
        conn: ConnId,
        user_type: ConnId,
    },

    Update{
        room: RoomId,
        msg: String,
        user_type: ConnId,
    },

    Send{
        room: RoomId,
        conn: ConnId,
        msg: String,
    }
}


#[derive(Debug)]
struct Room{
    id: RoomId,
    creation_time: i64,
    host: String,
    host_token: String,
    host_pipe: mpsc::UnboundedSender<Msg>,
    /// Map of connection IDs to their message receivers.
    sessions: HashMap<ConnId, mpsc::UnboundedSender<Msg>>,
}

impl Room{
    pub fn new(host: String) -> Self {
        let id = thread_rng().gen::<RoomId>();
        let creation_time = Utc::now().timestamp();
        let sessions = HashMap::new();
        //Generated HOST ID has a 256 bit length UUID
        let host_token = thread_rng().gen::<[u8; 32]>().to_vec().iter().map(|x| format!("{:02x}", x)).collect::<String>();

        Self{
            id,
            creation_time,
            host,
            host_token,
            host_pipe: mpsc::unbounded_channel().0,
            sessions,
        }
    }

    pub async fn add_client(&mut self, tx: mpsc::UnboundedSender<Msg>, user_type: ConnId) -> ConnId {

        if user_type == USER_HOST
        {
            self.host_pipe = tx;
            return 0;
        }
        // register session with random connection ID
        let id = thread_rng().gen::<ConnId>();
        log::info!("Adding client {} to room {}", id, self.id);
        self.sessions.insert(id, tx);

        id
    }

    pub async fn remove_client(&mut self, conn_id: ConnId, user_type: ConnId){
        if user_type == USER_HOST
        {
            self.host_pipe = mpsc::unbounded_channel().0;
            return;
        }
        log::info!("Removing client {} from room {}", conn_id, self.id);
        self.sessions.remove(&conn_id);
    }

    pub async fn broadcast(&self, msg: &str, user_type: ConnId){
        if user_type == USER_CLIENT
        {
            let _ = self.host_pipe.send(msg.to_owned());
            return;
        }
        for tx in self.sessions.values(){
            let _ = tx.send(msg.to_owned());
        }
    }

    pub async fn send(&self, conn_id: ConnId, msg: &str){
        let tx = self.sessions.get(&conn_id);
        if tx.is_none(){
            return;
        }
        let _ = tx.unwrap().send(msg.to_owned());
    }
}


#[derive(Debug)]
pub struct BingoServer {

    /// Map of room name to participant IDs in that room.
    rooms: HashMap<RoomId, Room>,

    /// Command receiver.
    cmd_rx: mpsc::UnboundedReceiver<Command>,
}

impl BingoServer{
    pub fn new() -> (Self, BingoServerHandle){
        let rooms = HashMap::with_capacity(0);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        (
            Self{
                rooms,
                cmd_rx,
            },
            BingoServerHandle{
                cmd_tx: cmd_tx.clone(),
            }
        )
    }

    pub async fn create_room(&mut self, host: String) -> RoomCreds {

        // Check if rooms contains a room with the same host
        for room in self.rooms.values(){
            if room.host == host{
                return RoomCreds::new(room.id, room.host_token.clone());
            }
        }

        let room= Room::new(host);
        let room_id = room.id;
        let room_token = room.host_token.clone();
        self.rooms.insert(room_id, room);

        RoomCreds::new(room_id, room_token)
    }

    pub async fn room_exists(&self, room_id: RoomId) -> bool {
        self.rooms.contains_key(&room_id)
    }

    pub async fn has_room_host_privileges(&self, room_id: RoomId, host_token: String) -> bool {
        let room = self.rooms.get(&room_id);
        match room {
            None => {return false;}
            Some(room) => {
                return room.host_token == host_token;
            }
        }
    }

    pub async fn add_client(&mut self, room_id: RoomId, tx: mpsc::UnboundedSender<Msg>, user_type: ConnId) -> ConnId {
        self.rooms.get_mut(&room_id).unwrap().add_client(tx, user_type).await
    }

    pub async fn remove_client(&mut self, room_id: RoomId, conn_id: ConnId, user_type: ConnId){
        self.rooms.get_mut(&room_id).unwrap().remove_client(conn_id, user_type).await;
    }

    pub async fn broadcast(&self, room_id: RoomId, msg: &str, user_type: ConnId){
        self.rooms.get(&room_id).unwrap().broadcast(msg, user_type).await;
    }

    pub async fn send(&self, room_id: RoomId, conn_id: ConnId, msg: &str){
        self.rooms.get(&room_id).unwrap().send(conn_id, msg).await;
    }

    pub async fn run(mut self) -> io::Result<()> {
        while let Some(cmd) = self.cmd_rx.recv().await {
            match cmd {
                Command::Create { host, res_tx } => {
                    let creds = self.create_room(host).await;
                    let _ = res_tx.send(creds);
                }

                Command::RoomExists { room_id, res_tx } => {
                    let exists = self.room_exists(room_id).await;
                    let _ = res_tx.send(exists);
                }

                Command::RoomHostAuth { room_id, host_token, res_tx } => {
                    let has_privileges = self.has_room_host_privileges(room_id, host_token).await;
                    let _ = res_tx.send(has_privileges);
                }

                Command::Connect { room, conn_tx, res_tx, user_type } => {
                    let conn_id = self.add_client(room, conn_tx, user_type).await;
                    let _ = res_tx.send(conn_id);
                }

                Command::Disconnect { room, conn, user_type } => {
                    self.remove_client(room, conn, user_type).await;
                }

                Command::Update { room, msg, user_type } => {
                    self.broadcast(room, &msg, user_type).await;
                }

                Command::Send { room, conn, msg } => {
                    self.send(room, conn, &msg).await;
                }
            }
        }

        Ok(())
    }
}


#[derive(Debug, Clone)]
pub struct BingoServerHandle {
    cmd_tx: mpsc::UnboundedSender<Command>,
}

impl BingoServerHandle {
    pub async fn create_room(&self, host: String) -> RoomCreds {
        let (res_tx, res_rx) = oneshot::channel();

        self.cmd_tx
            .send(Command::Create { host, res_tx })
            .unwrap();

        res_rx.await.unwrap()
    }

    pub async fn room_exists(&self, room_id: RoomId) -> bool {
        let (res_tx, res_rx) = oneshot::channel();

        self.cmd_tx
            .send(Command::RoomExists { room_id, res_tx })
            .unwrap();

        res_rx.await.unwrap()
    }

    pub async fn has_room_host_privileges(&self, room_id: RoomId, host_token: String) -> bool {
        let (res_tx, res_rx) = oneshot::channel();

        self.cmd_tx
            .send(Command::RoomHostAuth { room_id, host_token, res_tx })
            .unwrap();

        res_rx.await.unwrap()
    }

    pub async fn connect(&self, room: RoomId, conn_tx: mpsc::UnboundedSender<Msg>, user_type: ConnId ) -> ConnId {
        let (res_tx, res_rx) = oneshot::channel();

        self.cmd_tx
            .send(Command::Connect { room, conn_tx, res_tx, user_type })
            .unwrap();

        res_rx.await.unwrap()
    }

    pub async fn disconnect(&self, room: RoomId, conn: ConnId, user_type: ConnId) {
        self.cmd_tx.send(Command::Disconnect { room, conn, user_type }).unwrap();
    }

    pub async fn update(&self, room: RoomId, msg: String, user_type: ConnId){
        self.cmd_tx.send(Command::Update{room, msg, user_type}).unwrap();
    }

    pub async fn send(&self, room: RoomId, conn: ConnId, msg: String){
        self.cmd_tx.send(Command::Send{room, conn, msg}).unwrap();
    }
}
