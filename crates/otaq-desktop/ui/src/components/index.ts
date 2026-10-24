// The shared components (T-0217), drawn from *crewboard.pen → 01 · Components* (UIT-01: no native widgets). Screens
// import them from here: `import { Button, StatusPill } from "../../components";`. They take what a view model says
// (wording, enabled states, the generated `PillView`, `PriorityView`, `MenuItem`…) as typed props and keep only the
// moment of interaction (UIT-04). See the crate README, "Components"; the gallery shows each one.
import "./components.css";

export { Banner, type BannerAction, type BannerProps, type BannerTone } from "./Banner";
export {
  Button,
  IconButton,
  type ButtonProps,
  type ButtonVariant,
  type IconButtonProps,
  type ShownState,
} from "./Button";
export { CodeText } from "./CodeText";
export { DecisionItem, DecisionsList, type DecisionItemProps, type DecisionsListProps } from "./DecisionItem";
export {
  ConfirmDialog,
  Dialog,
  inOverlayHost,
  OVERLAY_HOST_ID,
  type ConfirmDialogProps,
  type DialogProps,
} from "./Dialog";
export { Checkbox, Field, Radio, TextArea, TextInput, type FieldProps } from "./Field";
export { type Anchor, useFloatingStyle, usePressOutside } from "./Floating";
export { Kbd } from "./Kbd";
export { LeadStatus, leadAction, openLeadTab, type LeadStatusProps } from "./LeadStatus";
export { AnchoredMenu, Menu, type AnchoredMenuProps, type MenuClose, type MenuProps } from "./Menu";
export {
  type InnerMotion,
  type MotionPreset,
  playEnterWhenReady,
  playLeave,
  playMotion,
  reducedMotion,
  useEnterExit,
  usePresence,
} from "./motion";
export { OtaqMark } from "./OtaqMark";
export { PLAIN_TEXT, plainTextOff, PROSE_TEXT } from "./plainText";
export { CountBadge, MetaChip, StatusPill, type ChipTone, type CountTone, type MetaChipProps } from "./Pill";
export { Popover, useDismiss, useEscape, type PopoverProps } from "./Popover";
export { PriorityChip, PriorityLabel } from "./Priority";
export { Select, type SelectProps } from "./Select";
export {
  StageChoices,
  TicketPath,
  shownChoice,
  type StageChoicesLook,
  type StageChoicesProps,
} from "./StageChoices";
export { Segmented, Tabs, Toggle, type TabIndicator, type TabItem, type TabsProps } from "./Tabs";
export { pickTheme, ThemePicker, useThemePicker, type ThemePickerProps } from "./ThemePicker";
export { Toast, type ToastIconTone, type ToastKind, type ToastProps } from "./Toast";
export { ToolAvatar, type ToolAvatarProps } from "./ToolAvatar";
export { ToolProblemCallout, type ToolProblemCalloutProps, type ToolProblemLook } from "./ToolProblem";
export { Tooltip, type TooltipProps } from "./Tooltip";
