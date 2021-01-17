// use crate::event_handlers::Handlers;
// use crate::filters::FilterCommand;
// use crate::statistics::StatCommand;
// use crate::traits::HandlerCommand;
// use tokio::sync::mpsc::UnboundedSender;
//
// /// todo
// #[derive(Default)]
// pub struct HandlersBuilder<H> {
//     transmitters: Vec<UnboundedSender<H>>,
// }
//
// /// todo
// impl<H: HandlerCommand> HandlersBuilder<H> {
//     /// todo
//     pub fn new() -> Self {
//         Self {
//             ..Default::default()
//         }
//     }
//
//     /// todo
//     pub fn transmitter(&mut self, transmitter: UnboundedSender<H>) -> &mut Self {
//         self.transmitters.push(transmitter);
//         self
//     }
//
//     /// todo
//     pub fn build(&self) -> Handlers {
//         // let tx_stats: UnboundedSender<StatCommand> = self.transmitters[0];
//         // let tx_filters: UnboundedSender<FilterCommand> = self.transmitters[1];
//
//         for tx in self.transmitters {}
//
//         Handlers {
//             handles: Vec::new(),
//             tx_stats,
//             tx_filters,
//         }
//     }
// }
