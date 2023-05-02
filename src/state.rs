use std::{time::Instant, ffi::OsString, sync::Arc, os::fd::AsRawFd};

use smithay::{wayland::{compositor::CompositorState, shell::xdg::{XdgShellState, decoration::XdgDecorationState}, shm::ShmState, output::OutputManagerState, data_device::DataDeviceState, socket::ListeningSocketSource}, reexports::{wayland_server::{Display, backend::{ClientData, ClientId, DisconnectReason}, DisplayHandle}, calloop::{LoopHandle, LoopSignal, generic::Generic, Interest, PostAction, Mode}}, input::{SeatState, Seat, keyboard::XkbConfig}, utils::{Logical, Point}, desktop::Window};

use crate::utils::workspace::Workspaces;

pub struct CalloopData<BackendData: Backend + 'static> {
    pub state: MagmaState<BackendData>,
    pub display: Display<MagmaState<BackendData>>,
}

pub trait Backend {
    fn seat_name(&self) -> String;
}

pub struct MagmaState<BackendData: Backend + 'static> {
    pub dh: DisplayHandle,
    pub backend_data: BackendData,
    pub start_time: Instant,
    pub loop_handle: LoopHandle<'static, CalloopData<BackendData>>,
    pub loop_signal: LoopSignal,

    // protocol state
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub xdg_decoration_state: XdgDecorationState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub data_device_state: DataDeviceState,
    pub seat_state: SeatState<MagmaState<BackendData>>,

    pub seat: Seat<Self>,
    pub seat_name: String,
    pub socket_name: OsString,

    pub workspaces: Workspaces,
    pub pointer_location: Point<f64, Logical>,
}

impl<BackendData: Backend> MagmaState<BackendData> {
    pub fn new(
        mut loop_handle: LoopHandle<'static, CalloopData<BackendData>>,
        loop_signal: LoopSignal,
        display: &mut Display<MagmaState<BackendData>>,
        backend_data: BackendData,
    ) -> Self {
        let start_time = Instant::now();

        let dh = display.handle();


        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let xdg_decoration_state = XdgDecorationState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&dh);
        let seat_name = backend_data.seat_name();
        let mut seat = seat_state.new_wl_seat(&dh, seat_name.clone());
        
        seat.add_keyboard(XkbConfig::default(), 200, 25).unwrap();
        seat.add_pointer();

        let workspaces = Workspaces::new(1);

        let socket_name = Self::init_wayland_listener(&mut loop_handle, display);

        Self {
            loop_handle,
            dh,
            backend_data,
            start_time,
            seat_name,
            socket_name,
            compositor_state,
            xdg_shell_state,
            xdg_decoration_state,
            loop_signal,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            seat,
            workspaces,
            pointer_location: Point::from((0.0, 0.0)),
        }
    }
    fn init_wayland_listener(
        handle: &mut LoopHandle<'static, CalloopData<BackendData>>,
        display: &mut Display<MagmaState<BackendData>>,
    ) -> OsString {
        // Creates a new listening socket, automatically choosing the next available `wayland` socket name.
        let listening_socket = ListeningSocketSource::new_auto().unwrap();

        // Get the name of the listening socket.
        // Clients will connect to this socket.
        let socket_name = listening_socket.socket_name().to_os_string();

        handle
            .insert_source(listening_socket, move |client_stream, _, state| {
                // Inside the callback, you should insert the client into the display.
                //
                // You may also associate some data with the client when inserting the client.
                state
                    .display
                    .handle()
                    .insert_client(client_stream, Arc::new(ClientState))
                    .unwrap();
            })
            .expect("Failed to init the wayland event source.");

        // You also need to add the display itself to the event loop, so that client events will be processed by wayland-server.
        handle
            .insert_source(
                Generic::new(
                    display.backend().poll_fd().as_raw_fd(),
                    Interest::READ,
                    Mode::Level,
                ),
                |_, _, state| {
                    state.display.dispatch_clients(&mut state.state).unwrap();
                    Ok(PostAction::Continue)
                },
            )
            .unwrap();

        socket_name
    }

    pub fn window_under(&mut self) -> Option<(Window, Point<i32, Logical>)> {
        let pos = self.pointer_location;
        self.workspaces
            .current()
            .window_under(pos)
            .map(|(w, p)| (w.clone(), p))
    }
}

pub struct ClientState;
impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}
