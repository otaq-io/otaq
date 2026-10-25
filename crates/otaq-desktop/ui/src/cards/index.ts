// The Ticket Card (T-0218), drawn from `TicketCardView` (`card_page.rs`). The
// board draws it in its columns; the gallery shows every variant. See the crate README, "Cards".
import "./cards.css";

export { type ChainState, type CardState, RESTING, stateOfLook } from "./look";
export { CheckRoundRows, stepLook } from "./CheckRound";
export { TicketCard, type TicketCardProps } from "./TicketCard";
