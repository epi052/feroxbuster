// use super::WildcardFilter;
// use crate::{
//     statistics::{
//         StatCommand::{self, UpdateUsizeField},
//         StatField::WildcardsFiltered,
//     },
//     FeroxResponse,
// };
// use anyhow::Result;
// use tokio::sync::mpsc::UnboundedSender;
//
// /// Simple helper to stay DRY; determines whether or not a given `FeroxResponse` should be reported
// /// to the user or not.
// pub fn should_filter_response(
//     response: &FeroxResponse,
//     tx_stats: UnboundedSender<StatCommand>,
// ) -> Result<bool> {
//     let filters = FILTERS
//     match FILTERS.read() {
//         Ok(filters) => {
//             for filter in filters.iter() {
//                 // wildcard.should_filter goes here
//                 if filter.should_filter_response(&response) {
//                     if filter.as_any().downcast_ref::<WildcardFilter>().is_some() {
//                         update_stat!(tx_stats, UpdateUsizeField(WildcardsFiltered, 1))
//                     }
//                     return true;
//                 }
//             }
//         }
//         Err(e) => {
//             log::error!("{}", e);
//         }
//     }
//     false
// }
