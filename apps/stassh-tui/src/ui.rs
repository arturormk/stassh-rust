use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use stassh_core::Host;
use stassh_core::openssh::{
    command_for_config, command_for_host, config_for_host_with_identity_path,
};
use uuid::Uuid;

use crate::app::{
    App, DeleteConfirmation, FolderSelectionState, Mode, StatusPageKey, TreeItemKind,
};
use crate::editor::{
    FolderEditor, FolderEditorMode, ForwardEditor, ForwardRowKind, HostEditor, HostEditorMode,
    IdentityEditor, JumpEditor,
};

pub(crate) fn draw_ui(frame: &mut Frame<'_>, app: &App) {
    let areas = ui_areas(frame.area());
    draw_tree(frame, app, areas.tree);
    draw_details(frame, app, areas.details);
    draw_status(frame, app, areas.status);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiAreas {
    pub(crate) tree: Rect,
    details: Rect,
    status: Rect,
}

pub(crate) fn ui_areas(area: Rect) -> UiAreas {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(area);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(root[0]);

    UiAreas {
        tree: body[0],
        details: body[1],
        status: root[1],
    }
}

fn draw_tree(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let title = match app.mode {
        Mode::Browse
        | Mode::EditHost
        | Mode::EditFolder
        | Mode::EditIdentity
        | Mode::EditJumps
        | Mode::EditForwards
        | Mode::Secrets
        | Mode::RevealSecret
        | Mode::ActionPalette
        | Mode::ConfirmDelete => "Hosts",
        Mode::PickMoveFolder => "Move To Folder",
        Mode::Search => "Search",
    };
    let visible_height = area.height.saturating_sub(2) as usize;
    let (items, offset, total_count) = match app.mode {
        Mode::Browse
        | Mode::EditHost
        | Mode::EditFolder
        | Mode::EditIdentity
        | Mode::EditJumps
        | Mode::EditForwards
        | Mode::Secrets
        | Mode::RevealSecret
        | Mode::ActionPalette
        | Mode::ConfirmDelete => {
            let all_items = app
                .tree
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let selected = index == app.selected;
                    let marker = if selected { "> " } else { "  " };
                    let icon = match item.kind {
                        TreeItemKind::Folder(folder_id) => folder_marker(app, folder_id),
                        TreeItemKind::Host(host_id) => {
                            if app.host_is_marked(host_id) {
                                "[x]"
                            } else {
                                "[ ]"
                            }
                        }
                    };
                    let line = format!("{marker}{}{icon} {}", "  ".repeat(item.depth), item.label);
                    list_item(line, selected)
                })
                .collect::<Vec<_>>();
            let offset = selected_scroll_offset(app.selected, visible_height);
            (
                visible_items(all_items, offset, visible_height),
                offset,
                app.tree.len(),
            )
        }
        Mode::PickMoveFolder => {
            let folders = app.folder_picker_items();
            if folders.is_empty() {
                (vec![ListItem::new("  no folders")], 0, 0)
            } else {
                let total_count = folders.len();
                let all_items = folders
                    .iter()
                    .enumerate()
                    .map(|(index, folder)| {
                        let selected = index == app.move_folder_selected;
                        let marker = if selected { "> " } else { "  " };
                        list_item(
                            format!("{marker}{}[+] {}", "  ".repeat(folder.depth), folder.label),
                            selected,
                        )
                    })
                    .collect::<Vec<_>>();
                let offset = selected_scroll_offset(app.move_folder_selected, visible_height);
                (
                    visible_items(all_items, offset, visible_height),
                    offset,
                    total_count,
                )
            }
        }
        Mode::Search => {
            let matches = app.search_matches();
            if matches.is_empty() {
                (vec![ListItem::new("  no matches")], 0, 0)
            } else {
                let total_count = matches.len();
                let all_items = matches
                    .iter()
                    .enumerate()
                    .map(|(index, host)| {
                        let selected = index == app.search_selected;
                        let marker = if selected { "> " } else { "  " };
                        let select_marker = if app.host_is_marked(host.id) {
                            "[x]"
                        } else {
                            "[ ]"
                        };
                        list_item(
                            format!(
                                "{marker}{select_marker} {}  {}",
                                app.vault.host_path(host),
                                host.hostname
                            ),
                            selected,
                        )
                    })
                    .collect::<Vec<_>>();
                let offset = selected_scroll_offset(app.search_selected, visible_height);
                (
                    visible_items(all_items, offset, visible_height),
                    offset,
                    total_count,
                )
            }
        }
    };
    let title = if total_count > visible_height && visible_height > 0 {
        format!(
            "{title} {}-{} of {}",
            offset + 1,
            (offset + items.len()).min(total_count),
            total_count
        )
    } else {
        title.to_string()
    };

    frame.render_widget(
        List::new(items).block(Block::default().title(title).borders(Borders::ALL)),
        area,
    );
}

fn folder_marker(app: &App, folder_id: Uuid) -> &'static str {
    if app.folder_is_collapsed(folder_id) {
        return "[>]";
    }
    match app.folder_selection_state(folder_id) {
        FolderSelectionState::Empty | FolderSelectionState::None => "[v]",
        FolderSelectionState::Some => "[-]",
        FolderSelectionState::All => "[x]",
    }
}

fn visible_items(
    items: Vec<ListItem<'static>>,
    offset: usize,
    visible_height: usize,
) -> Vec<ListItem<'static>> {
    if visible_height == 0 || items.len() <= visible_height {
        return items;
    }
    items
        .into_iter()
        .skip(offset)
        .take(visible_height)
        .collect()
}

fn selected_scroll_offset(selected: usize, visible_height: usize) -> usize {
    if visible_height == 0 {
        0
    } else {
        selected.saturating_sub(visible_height - 1)
    }
}

fn list_item(text: String, selected: bool) -> ListItem<'static> {
    let style = if selected {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    ListItem::new(text).style(style)
}

fn draw_details(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if let Some(pending_delete) = &app.pending_delete {
        draw_delete_confirmation(frame, pending_delete, area);
        return;
    }

    if let Some(editor) = &app.editor {
        draw_editor(frame, editor, area);
        return;
    }

    if let Some(editor) = &app.folder_editor {
        draw_folder_editor(frame, editor, area);
        return;
    }

    if let Some(editor) = &app.identity_editor {
        draw_identity_editor(frame, editor, app, area);
        return;
    }

    if let Some(editor) = &app.jump_editor {
        draw_jump_editor(frame, editor, area);
        return;
    }

    if let Some(editor) = &app.forward_editor {
        draw_forward_editor(frame, editor, area);
        return;
    }

    if app.mode == Mode::RevealSecret {
        draw_reveal_prompt(frame, app, area);
        return;
    }

    if app.mode == Mode::Secrets {
        draw_secrets_view(frame, app, area);
        return;
    }

    if app.mode == Mode::PickMoveFolder {
        draw_move_folder_picker(frame, app, area);
        return;
    }

    if app.mode == Mode::ActionPalette {
        draw_action_palette(frame, app, area);
        return;
    }

    let lines = if let Some(host) = app.selected_host() {
        host_detail_lines(app, host)
    } else if let Some(folder_id) = app.selected_folder_id() {
        folder_detail_lines(app, folder_id)
    } else {
        vec![Line::from("No selection")]
    };

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Details").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_reveal_prompt(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(prompt) = &app.reveal_prompt else {
        frame.render_widget(
            Paragraph::new("No reveal in progress").block(
                Block::default()
                    .title("Reveal Secret")
                    .borders(Borders::ALL),
            ),
            area,
        );
        return;
    };
    let masked = "*".repeat(prompt.password.chars().count());
    let lines = vec![
        Line::from(Span::styled(
            "Reveal Secret",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        field("Field", &prompt.field),
        field("Master", &masked),
        Line::from(""),
        Line::from("Press Enter to reveal. Press Esc to cancel."),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title("Reveal Secret")
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_secrets_view(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(view) = &app.secrets_view else {
        frame.render_widget(
            Paragraph::new("No secrets view")
                .block(Block::default().title("Secrets").borders(Borders::ALL)),
            area,
        );
        return;
    };
    let Some(store) = &app.secrets_store else {
        frame.render_widget(
            Paragraph::new(format!(
                "Secrets store not found: {}",
                app.secrets_path.display()
            ))
            .block(Block::default().title("Secrets").borders(Borders::ALL)),
            area,
        );
        return;
    };
    let set = match store.set(&view.set_key) {
        Ok(set) => set,
        Err(error) => {
            frame.render_widget(
                Paragraph::new(format!("{error}"))
                    .block(Block::default().title("Secrets").borders(Borders::ALL)),
                area,
            );
            return;
        }
    };
    let visible_rows = area.height.saturating_sub(8) as usize;
    let offset = selected_scroll_offset(view.selected, visible_rows.max(1));
    let title = if view.fields.len() > visible_rows && visible_rows > 0 {
        format!(
            "Secrets {}-{} of {}",
            offset + 1,
            (offset + visible_rows).min(view.fields.len()),
            view.fields.len()
        )
    } else {
        "Secrets".to_string()
    };
    let mut lines = vec![
        field("Host", &view.host_path),
        field("Set", &view.set_key),
        field("Label", view.set_label.as_deref().unwrap_or("")),
        Line::from(""),
    ];
    if view.fields.is_empty() {
        lines.push(Line::from("No fields in this secrets set."));
    } else {
        for (index, field_name) in view
            .fields
            .iter()
            .enumerate()
            .skip(offset)
            .take(visible_rows.max(1))
        {
            let selected = index == view.selected;
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let marker = if selected { "> " } else { "  " };
            let value = match set.fields.get(field_name) {
                Some(stassh_core::SecretField::Plain(value)) => value.as_str(),
                Some(stassh_core::SecretField::Secret(_)) => view
                    .revealed
                    .as_ref()
                    .filter(|revealed| revealed.field == *field_name)
                    .and_then(|revealed| revealed.plaintext.expose_str().ok())
                    .unwrap_or("[secret]"),
                None => "(missing)",
            };
            lines.push(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(format!("{:>16}: ", field_name), style),
                Span::styled(value, style),
            ]));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from("Enter reveal | h hide | Esc close"));
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_move_folder_picker(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let selected_folder = app
        .folder_picker_items()
        .get(app.move_folder_selected)
        .map(|folder| folder.path.clone())
        .unwrap_or_else(|| "(none)".to_string());
    let lines = vec![
        Line::from(Span::styled(
            "Move selected hosts",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        field("Hosts", &app.pending_move_count().to_string()),
        field("Target", &selected_folder),
        Line::from(""),
        Line::from("Press Enter to move. Press Esc to cancel."),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Move Hosts").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_action_palette(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(host) = app.selected_host() else {
        frame.render_widget(
            Paragraph::new("No host selected")
                .block(Block::default().title("Actions").borders(Borders::ALL)),
            area,
        );
        return;
    };

    let block = Block::default().title("Actions").borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let actions = app.selected_actions();
    let detail_height = if actions.is_empty() || inner.height < 8 {
        0
    } else {
        4
    };
    let areas = if detail_height == 0 {
        vec![inner]
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(detail_height)])
            .split(inner)
            .to_vec()
    };

    let mut lines = vec![
        field("Host", &app.vault.host_path(host)),
        Line::from(""),
        Line::from(Span::styled(
            "Actions",
            Style::default().add_modifier(Modifier::BOLD),
        )),
    ];
    if actions.is_empty() {
        lines.push(Line::from("No actions"));
    } else {
        let visible_rows = areas[0].height.saturating_sub(3) as usize;
        let offset = selected_scroll_offset(app.action_selected, visible_rows);
        for (index, action) in actions.iter().enumerate().skip(offset).take(visible_rows) {
            let marker = if index == app.action_selected {
                "> "
            } else {
                "  "
            };
            let style = if index == app.action_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            lines.push(Line::from(Span::styled(
                format!("{marker}{}", action.name),
                style,
            )));
        }
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), areas[0]);

    if detail_height > 0
        && let Some(action) = actions.get(app.action_selected)
    {
        frame.render_widget(
            Paragraph::new(action_detail_lines(action)).wrap(Wrap { trim: false }),
            areas[1],
        );
    }
}

fn action_detail_lines(action: &stassh_core::ActionDefinition) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "Selected",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        field(
            "Remote",
            action.remote_command.as_deref().unwrap_or("(none)"),
        ),
        field("Forwards", &action.forwards.len().to_string()),
        field(
            "Local",
            action
                .local_launch
                .as_ref()
                .and_then(|command| command.capability.as_deref().or(command.program.as_deref()))
                .unwrap_or("(none)"),
        ),
    ]
}

fn draw_delete_confirmation(
    frame: &mut Frame<'_>,
    pending_delete: &DeleteConfirmation,
    area: Rect,
) {
    let (title, lines) = match pending_delete {
        DeleteConfirmation::Host { path, hostname, .. } => (
            "Delete Host",
            vec![
                Line::from(Span::styled(
                    "Delete this host?",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                field("Path", path),
                field("HostName", hostname),
                Line::from(""),
                Line::from(
                    "This removes the host from the vault and from other hosts' jump chains.",
                ),
                Line::from(""),
                Line::from("Press y or Enter to delete. Press n or Esc to cancel."),
            ],
        ),
        DeleteConfirmation::Folder { path, .. } => (
            "Delete Folder",
            vec![
                Line::from(Span::styled(
                    "Delete this folder?",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                field("Path", path),
                Line::from(""),
                Line::from("Only empty folders can be deleted."),
                Line::from(""),
                Line::from("Press y or Enter to delete. Press n or Esc to cancel."),
            ],
        ),
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_editor(frame: &mut Frame<'_>, editor: &HostEditor, area: Rect) {
    let mut lines = vec![
        Line::from(Span::styled(
            editor_title(editor),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("Ctrl+S save | Esc cancel | Tab/Down next | Shift+Tab/Up previous"),
        Line::from(""),
    ];
    for (index, field) in editor.fields.iter().enumerate() {
        let selected = index == editor.selected;
        let label_style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };
        let value_style = if selected {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{:>8}: ", field.label), label_style),
            Span::styled(field.value.clone(), value_style),
        ]));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(editor_title(editor))
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_folder_editor(frame: &mut Frame<'_>, editor: &FolderEditor, area: Rect) {
    let mut lines = vec![
        Line::from(Span::styled(
            folder_editor_title(editor),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("Ctrl+S save | Esc cancel | Tab/Down next | Shift+Tab/Up previous"),
        Line::from(""),
    ];
    for (index, field) in editor.fields.iter().enumerate() {
        let selected = index == editor.selected;
        let label_style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };
        let value_style = if selected {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{:>8}: ", field.label), label_style),
            Span::styled(field.value.clone(), value_style),
        ]));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(folder_editor_title(editor))
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_identity_editor(frame: &mut Frame<'_>, editor: &IdentityEditor, app: &App, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let fixed_lines = 6;
    let visible_choices = inner_height.saturating_sub(fixed_lines).max(1) / 2;
    let visible_choices = visible_choices.max(1);
    let offset = selected_scroll_offset(editor.selected, visible_choices);
    let title = if editor.choices.len() > visible_choices {
        format!(
            "Edit Identity {}-{} of {}",
            offset + 1,
            (offset + visible_choices).min(editor.choices.len()),
            editor.choices.len()
        )
    } else {
        "Edit Identity".to_string()
    };
    let current = editor
        .original_fingerprint
        .as_deref()
        .map(|fingerprint| identity_summary(app, fingerprint))
        .unwrap_or_else(|| "(none)".to_string());
    let mut lines = vec![
        Line::from(Span::styled(
            "Edit Identity",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("Ctrl+S save | Esc cancel | Up/Down choose | Home none | End last"),
        Line::from(""),
        field("Host", &editor.host_path),
        field("Current", &current),
        Line::from(""),
    ];
    for (index, choice) in editor
        .choices
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible_choices)
    {
        let selected = index == editor.selected;
        let style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };
        let marker = if selected { ">" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} "), style),
            Span::styled(choice.label.clone(), style),
        ]));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(choice.detail.clone(), Style::default().fg(Color::DarkGray)),
        ]));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_jump_editor(frame: &mut Frame<'_>, editor: &JumpEditor, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let fixed_lines = 5;
    let visible_choices = inner_height.saturating_sub(fixed_lines).max(1) / 2;
    let visible_choices = visible_choices.max(1);
    let offset = selected_scroll_offset(editor.selected, visible_choices);
    let title = if editor.choices.len() > visible_choices {
        format!(
            "Edit Jumps {}-{} of {}",
            offset + 1,
            (offset + visible_choices).min(editor.choices.len()),
            editor.choices.len()
        )
    } else {
        "Edit Jumps".to_string()
    };
    let mut lines = vec![
        Line::from(Span::styled(
            "Edit Jumps",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("Ctrl+S save | Esc cancel | Space toggle | [/ ] reorder chosen"),
        Line::from(""),
        field("Host", &editor.host_path),
        Line::from(""),
    ];
    if editor.choices.is_empty() {
        lines.push(Line::from("No other hosts are available as jump targets."));
    } else {
        for (index, choice) in editor
            .choices
            .iter()
            .enumerate()
            .skip(offset)
            .take(visible_choices)
        {
            let selected = index == editor.selected;
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let marker = if selected { ">" } else { " " };
            let checkbox = if choice.chosen { "[x]" } else { "[ ]" };
            lines.push(Line::from(vec![
                Span::styled(format!("{marker} {checkbox} "), style),
                Span::styled(choice.label.clone(), style),
            ]));
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(choice.detail.clone(), Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_forward_editor(frame: &mut Frame<'_>, editor: &ForwardEditor, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let fixed_lines = 6;
    let visible_rows = inner_height.saturating_sub(fixed_lines).max(1) / 2;
    let visible_rows = visible_rows.max(1);
    let offset = selected_scroll_offset(editor.selected_row, visible_rows);
    let title = if editor.rows.len() > visible_rows {
        format!(
            "Edit Forwards {}-{} of {}",
            offset + 1,
            (offset + visible_rows).min(editor.rows.len()),
            editor.rows.len()
        )
    } else {
        "Edit Forwards".to_string()
    };
    let mut lines = vec![
        Line::from(Span::styled(
            "Edit Forwards",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("Ctrl+S save | Esc cancel | a local | A remote | d dynamic | x delete"),
        Line::from("Tab fields | Up/Down rows | Home clear field | End last row"),
        Line::from(""),
        field("Host", &editor.host_path),
        Line::from(""),
    ];
    if editor.rows.is_empty() {
        lines.push(Line::from("No forwards. Press a, A, or d to add one."));
    } else {
        for (row_index, row) in editor
            .rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(visible_rows)
        {
            let row_selected = row_index == editor.selected_row;
            let row_style = if row_selected {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let marker = if row_selected { ">" } else { " " };
            lines.push(Line::from(vec![
                Span::styled(format!("{marker} "), row_style),
                Span::styled(forward_kind_label(row.kind), row_style),
            ]));
            let mut spans = vec![Span::raw("  ")];
            for (field_index, field) in row.fields.iter().enumerate() {
                let selected = row_selected && field_index == editor.selected_field;
                let style = if selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                spans.push(Span::styled(format!("{}=", field.label), style));
                spans.push(Span::styled(field.value.clone(), style));
                spans.push(Span::raw("  "));
            }
            lines.push(Line::from(spans));
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn forward_kind_label(kind: ForwardRowKind) -> &'static str {
    match kind {
        ForwardRowKind::Local => "Local",
        ForwardRowKind::Remote => "Remote",
        ForwardRowKind::Dynamic => "Dynamic",
    }
}

fn editor_title(editor: &HostEditor) -> &'static str {
    match editor.mode {
        HostEditorMode::Edit { .. } => "Edit Host",
        HostEditorMode::Create { .. } => "Create Host",
    }
}

fn folder_editor_title(editor: &FolderEditor) -> &'static str {
    match editor.mode {
        FolderEditorMode::Edit { .. } => "Edit Folder",
        FolderEditorMode::Create { .. } => "Create Folder",
    }
}

fn host_detail_lines(app: &App, host: &Host) -> Vec<Line<'static>> {
    let resolved = match app
        .vault
        .resolve_host(stassh_core::HostSelector::Id(host.id))
    {
        Ok(resolved) => resolved,
        Err(error) => return vec![Line::from(format!("failed to resolve host: {error}"))],
    };

    let mut lines = vec![
        field("Path", &resolved.path),
        field("HostName", &resolved.hostname),
        field("Port", &resolved.port.to_string()),
        field("User", resolved.username.as_deref().unwrap_or("(default)")),
        field("Secrets", resolved.secrets.as_deref().unwrap_or("(none)")),
        field("Tags", &display_list(&resolved.tags)),
        field("Notes", resolved.notes.as_deref().unwrap_or("")),
    ];

    match &resolved.identity_fingerprint {
        Some(fingerprint) => {
            lines.push(field("Identity", &identity_summary(app, fingerprint)));
            lines.push(field("Fingerprint", fingerprint));
            match app.local_config.identity_path(fingerprint) {
                Some(path) if path.exists() => {
                    lines.push(field("Identity path", &path.display().to_string()))
                }
                Some(path) => lines.push(field(
                    "Identity path",
                    &format!("missing: {}", path.display()),
                )),
                None => lines.push(field("Identity path", "(unmapped)")),
            }
        }
        None => lines.push(field("Identity", "(none - password/default)")),
    }

    lines.push(field(
        "Jump chain",
        &display_jump_chain(&resolved.jump_chain),
    ));
    lines.push(field(
        "Forwards",
        &display_list(
            &resolved
                .forwards
                .iter()
                .map(display_forward)
                .collect::<Vec<_>>(),
        ),
    ));
    lines.push(field(
        "Actions",
        &display_list(
            &app.selected_actions()
                .iter()
                .map(|action| action.name.to_string())
                .collect::<Vec<_>>(),
        ),
    ));
    lines.push(field("SSH options", &display_list(&resolved.ssh_options)));

    if app.show_diagnostics {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Diagnostics",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(field(
            "OpenSSH command",
            &command_for_host(&resolved).render_for_display(),
        ));
        let identity_path = resolved
            .identity_fingerprint
            .as_deref()
            .and_then(|fingerprint| app.local_config.identity_path(fingerprint));
        let config = config_for_host_with_identity_path(&resolved, identity_path);
        lines.push(field(
            "Config command",
            &command_for_config("<temporary-config>", &config.alias).render_for_display(),
        ));
    }

    lines
}

fn identity_summary(app: &App, fingerprint: &str) -> String {
    match app
        .local_config
        .identity_mappings
        .iter()
        .find(|mapping| mapping.fingerprint == fingerprint)
    {
        Some(mapping) => mapping
            .preferred_name
            .as_deref()
            .unwrap_or(fingerprint)
            .to_string(),
        None => format!("unmapped: {fingerprint}"),
    }
}

fn folder_detail_lines(app: &App, folder_id: Uuid) -> Vec<Line<'static>> {
    let path = app.vault.folder_path(folder_id);
    let direct_hosts = app
        .vault
        .hosts
        .iter()
        .filter(|host| host.folder_id == folder_id)
        .count();
    let child_folders = app
        .vault
        .folders
        .iter()
        .filter(|folder| folder.parent_id == Some(folder_id))
        .count();
    vec![
        field("ID", &folder_id.to_string()),
        field("Folder", &path),
        field(
            "Parent ID",
            &app.vault
                .folder(folder_id)
                .and_then(|folder| folder.parent_id)
                .map(|id| id.to_string())
                .unwrap_or_else(|| "(none)".to_string()),
        ),
        field("Child folders", &child_folders.to_string()),
        field("Direct hosts", &direct_hosts.to_string()),
    ]
}

fn draw_status(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let pages = status_pages(app, area.width.saturating_sub(2) as usize);
    let page_count = pages.len().max(1);
    let page_index = status_page_index(app.status_page, page_count);
    let title = status_title(page_index, page_count, app.status_page_key);
    let text = pages.get(page_index).cloned().unwrap_or_default();

    frame.render_widget(
        Paragraph::new(text).block(Block::default().title(title).borders(Borders::ALL)),
        area,
    );
}

fn status_title(page_index: usize, page_count: usize, key: StatusPageKey) -> String {
    if page_count > 1 {
        format!(
            "Status {}/{} ({} more)",
            page_index + 1,
            page_count,
            key.label()
        )
    } else {
        "Status".to_string()
    }
}

fn status_page_index(status_page: usize, page_count: usize) -> usize {
    status_page % page_count.max(1)
}

fn status_pages(app: &App, width: usize) -> Vec<String> {
    logical_status_lines(app)
        .into_iter()
        .flat_map(|line| wrap_status_line(&line, width))
        .collect()
}

fn logical_status_lines(app: &App) -> Vec<String> {
    let mode = match app.mode {
        Mode::Browse => "browse",
        Mode::Search => "search",
        Mode::EditHost => "edit",
        Mode::EditFolder => "edit",
        Mode::EditIdentity => "identity",
        Mode::EditJumps => "jumps",
        Mode::EditForwards => "forwards",
        Mode::Secrets => "secrets",
        Mode::RevealSecret => "reveal",
        Mode::ActionPalette => "actions",
        Mode::ConfirmDelete => "delete",
        Mode::PickMoveFolder => "move",
    };
    let query = if app.mode == Mode::Search {
        format!(" | query: {}", app.search)
    } else {
        String::new()
    };
    let diagnostics = if app.show_diagnostics {
        " | diagnostics:on"
    } else {
        ""
    };
    let tmux = if app.tmux_available { "on" } else { "off" };
    let help = match app.mode {
        Mode::EditHost | Mode::EditFolder => "Ctrl+S save | Esc cancel | Tab/Shift+Tab fields",
        Mode::EditIdentity => "Ctrl+S save | Esc cancel | Up/Down choose identity | Home none",
        Mode::EditJumps => "Ctrl+S save | Esc cancel | Space toggle jump | [/ ] reorder chosen",
        Mode::EditForwards => {
            "Ctrl+S save | Esc cancel | a local | A remote | d dynamic | x delete | Tab fields"
        }
        Mode::Secrets => "Enter reveal | h hide | Esc close | j/k choose field | Home/End",
        Mode::RevealSecret => "Enter reveal | Esc cancel | type master password",
        Mode::ActionPalette => "Enter run action | Esc cancel | j/k choose action | Home/End",
        Mode::ConfirmDelete => "y/Enter delete | n/Esc cancel",
        Mode::PickMoveFolder => "Enter move | Esc cancel | j/k choose folder | Home/End",
        _ => {
            "q quit | / search | Space select | u clear | m move | n new host | C copy host | f new folder | e edit | i identity | J jumps | F forwards | a actions | s secrets | x delete | Enter connect | t tmux window | d diagnostics | r reload | Home/End/PgDn siblings | PgUp parent"
        }
    };
    let mut lines = vec![
        help.to_string(),
        format!(
            "mode:{mode}{query}{diagnostics} | selected:{} | tmux:{tmux}",
            app.selected_count()
        ),
        format!("vault:{}", app.vault_path.display()),
        format!("local:{}", app.local_config_path.display()),
        format!("secrets:{}", app.secrets_path.display()),
    ];
    if !app.status.is_empty() {
        lines.push(app.status.clone());
    }
    lines
}

fn wrap_status_line(line: &str, width: usize) -> Vec<String> {
    if line.is_empty() {
        return Vec::new();
    }
    if width == 0 {
        return vec![String::new()];
    }

    let mut pages = Vec::new();
    let mut remaining = line.trim();
    while remaining.len() > width {
        let split = remaining[..width]
            .rfind(' ')
            .filter(|split| *split > 0)
            .unwrap_or(width);
        pages.push(remaining[..split].trim_end().to_string());
        remaining = remaining[split..].trim_start();
    }
    if !remaining.is_empty() {
        pages.push(remaining.to_string());
    }
    pages
}

fn field(label: &'static str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(value.to_string()),
    ])
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        "(none)".to_string()
    } else {
        values.join(", ")
    }
}

fn display_jump_chain(jumps: &[stassh_core::model::ResolvedJump]) -> String {
    if jumps.is_empty() {
        "(none)".to_string()
    } else {
        jumps
            .iter()
            .map(|jump| jump.display_name.as_str())
            .collect::<Vec<_>>()
            .join(" -> ")
    }
}

fn display_forward(forward: &stassh_core::ForwardDefinition) -> String {
    match forward {
        stassh_core::ForwardDefinition::Local {
            bind_address,
            local_port,
            destination_host,
            destination_port,
        } => format!("L {bind_address}:{local_port} -> {destination_host}:{destination_port}"),
        stassh_core::ForwardDefinition::Remote {
            bind_address,
            remote_port,
            destination_host,
            destination_port,
        } => format!("R {bind_address}:{remote_port} -> {destination_host}:{destination_port}"),
        stassh_core::ForwardDefinition::Dynamic {
            bind_address,
            local_port,
        } => format!("D {bind_address}:{local_port}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_offset_keeps_selected_row_visible() {
        assert_eq!(selected_scroll_offset(0, 5), 0);
        assert_eq!(selected_scroll_offset(4, 5), 0);
        assert_eq!(selected_scroll_offset(5, 5), 1);
        assert_eq!(selected_scroll_offset(20, 5), 16);
    }

    #[test]
    fn scroll_offset_handles_empty_viewport() {
        assert_eq!(selected_scroll_offset(10, 0), 0);
    }

    #[test]
    fn wraps_short_status_line_to_one_page() {
        assert_eq!(wrap_status_line("short text", 20), vec!["short text"]);
    }

    #[test]
    fn wraps_long_status_line_to_multiple_pages() {
        assert_eq!(
            wrap_status_line("alpha beta gamma delta", 12),
            vec!["alpha beta", "gamma delta"]
        );
    }

    #[test]
    fn status_pages_omit_empty_transient_status() {
        let mut app = test_app();
        app.status = String::new();

        let pages = status_pages(&app, 200);

        assert!(pages.iter().all(|page| !page.is_empty()));
        assert!(pages.iter().all(|page| page != "identity saved"));
    }

    #[test]
    fn status_pages_include_transient_status_when_present() {
        let mut app = test_app();
        app.status = "identity saved".to_string();

        let pages = status_pages(&app, 200);

        assert!(pages.iter().any(|page| page == "identity saved"));
    }

    #[test]
    fn status_page_index_wraps_past_last_page() {
        assert_eq!(status_page_index(0, 5), 0);
        assert_eq!(status_page_index(4, 5), 4);
        assert_eq!(status_page_index(5, 5), 0);
        assert_eq!(status_page_index(6, 5), 1);
    }

    #[test]
    fn status_page_index_handles_empty_page_count() {
        assert_eq!(status_page_index(10, 0), 0);
    }

    #[test]
    fn status_title_includes_f1_hint_for_multiple_pages() {
        assert_eq!(
            status_title(0, 3, StatusPageKey::F1),
            "Status 1/3 (F1 more)"
        );
        assert_eq!(
            status_title(2, 3, StatusPageKey::F1),
            "Status 3/3 (F1 more)"
        );
        assert_eq!(
            status_title(0, 3, StatusPageKey::CtrlG),
            "Status 1/3 (Ctrl-G more)"
        );
    }

    #[test]
    fn status_title_omits_f1_hint_for_single_page() {
        assert_eq!(status_title(0, 1, StatusPageKey::CtrlG), "Status");
    }

    fn test_app() -> App {
        App::new(
            std::path::PathBuf::from("/tmp/vault.json"),
            std::path::PathBuf::from("/tmp/local.json"),
            std::path::PathBuf::from("/tmp/secrets.json"),
            stassh_core::Vault::new(),
            stassh_core::LocalConfig::new(),
            None,
            false,
            false,
            StatusPageKey::F1,
        )
    }
}
