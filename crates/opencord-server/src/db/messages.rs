use sqlx::SqliteConnection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRow {
    pub id: i64,
    pub channel_id: i64,
    pub author_id: i64,
    pub content: String,
    pub edited_at: Option<i64>,
    pub deleted: bool,
    /// For a command's public result: the command run.
    pub interaction_command: Option<String>,
    /// Who ran it; empty on rows from before this was kept.
    pub interaction_user_id: Option<i64>,
    /// `CommandState` in the protocol.
    pub command_state: i64,
}

/// The command a message is the result of.
#[derive(Debug, Clone, Copy)]
pub struct CommandRun<'a> {
    pub name: &'a str,
    pub user_id: i64,
    /// `CommandState` in the protocol.
    pub state: i32,
}

/// `CommandState::Thinking`.
const THINKING: i64 = 1;
/// `CommandState::NoAnswer`.
const NO_ANSWER: i64 = 2;

pub async fn insert(
    conn: &mut SqliteConnection,
    id: i64,
    channel_id: i64,
    author_id: i64,
    content: &str,
) -> Result<(), sqlx::Error> {
    insert_result(conn, id, channel_id, author_id, content, None).await
}

/// A message, or a command's public result with the command run.
pub async fn insert_result(
    conn: &mut SqliteConnection,
    id: i64,
    channel_id: i64,
    author_id: i64,
    content: &str,
    run: Option<CommandRun<'_>>,
) -> Result<(), sqlx::Error> {
    let command = run.map(|run| run.name);
    let user_id = run.map(|run| run.user_id);
    let state = run.map_or(0, |run| i64::from(run.state));
    sqlx::query!(
        "INSERT INTO messages
             (id, channel_id, author_id, content, interaction_command, interaction_user_id,
              command_state)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
        id,
        channel_id,
        author_id,
        content,
        command,
        user_id,
        state
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A message that has not been deleted.
pub async fn find(conn: &mut SqliteConnection, id: i64) -> Result<Option<MessageRow>, sqlx::Error> {
    sqlx::query_as!(
        MessageRow,
        r#"SELECT id, channel_id, author_id, content, edited_at, deleted AS "deleted: bool",
                  interaction_command, interaction_user_id, command_state
           FROM messages WHERE id = $1 AND deleted = FALSE"#,
        id
    )
    .fetch_optional(conn)
    .await
}

pub async fn update_content(
    conn: &mut SqliteConnection,
    id: i64,
    content: &str,
    edited_at: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE messages SET content = $1, edited_at = $2 WHERE id = $3",
        content,
        edited_at,
        id
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// A thinking message gets its reply: new text, no "edited" mark. False if
/// it was deleted or has already finished.
pub async fn finish_thinking(
    conn: &mut SqliteConnection,
    id: i64,
    content: &str,
    state: i32,
) -> Result<bool, sqlx::Error> {
    let state = i64::from(state);
    let done = sqlx::query!(
        "UPDATE messages SET content = $1, command_state = $2
         WHERE id = $3 AND command_state = $4 AND deleted = FALSE",
        content,
        state,
        id,
        THINKING
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// After a restart nothing can answer the commands left thinking.
pub async fn sweep_thinking(conn: &mut SqliteConnection) -> Result<u64, sqlx::Error> {
    let done = sqlx::query!(
        "UPDATE messages SET command_state = $1,
             content = (SELECT display_name FROM users WHERE users.id = messages.author_id)
                 || ' didn''t answer'
         WHERE command_state = $2",
        NO_ANSWER,
        THINKING
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected())
}

pub async fn mark_deleted(conn: &mut SqliteConnection, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!("UPDATE messages SET deleted = TRUE WHERE id = $1", id)
        .execute(conn)
        .await?;
    Ok(())
}

/// Newest first, only ids below `before`.
pub async fn list(
    conn: &mut SqliteConnection,
    channel_id: i64,
    before: i64,
    limit: i64,
) -> Result<Vec<MessageRow>, sqlx::Error> {
    sqlx::query_as!(
        MessageRow,
        r#"SELECT id, channel_id, author_id, content, edited_at, deleted AS "deleted: bool",
                  interaction_command, interaction_user_id, command_state
           FROM messages
           WHERE channel_id = $1 AND id < $2 AND deleted = FALSE
           ORDER BY id DESC
           LIMIT $3"#,
        channel_id,
        before,
        limit
    )
    .fetch_all(conn)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_pool;
    use sqlx::SqlitePool;

    const CHANNEL: i64 = 10;
    const PLUGIN_USER: i64 = 20;
    const MEMBER: i64 = 21;
    const THINKING: i32 = 1;
    const NO_ANSWER: i32 = 2;
    const ANSWERED: i32 = 0;

    async fn seed(pool: &SqlitePool) {
        for (id, name) in [(PLUGIN_USER, "Dice Bot"), (MEMBER, "Kai")] {
            sqlx::query(
                "INSERT INTO users (id, public_key, display_name, created_at) VALUES (?, ?, ?, 0)",
            )
            .bind(id)
            .bind(vec![id as u8; 32])
            .bind(name)
            .execute(pool)
            .await
            .unwrap();
        }
        sqlx::query("INSERT INTO channels (id, kind, name, position, created_at) VALUES (?, 'text', 'general', 0, 0)")
            .bind(CHANNEL)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn thinking(pool: &SqlitePool, id: i64) {
        let run = CommandRun {
            name: "roll",
            user_id: MEMBER,
            state: THINKING,
        };
        let mut conn = pool.acquire().await.unwrap();
        insert_result(
            &mut conn,
            id,
            CHANNEL,
            PLUGIN_USER,
            "Dice Bot is thinking…",
            Some(run),
        )
        .await
        .unwrap();
    }

    async fn row(pool: &SqlitePool, id: i64) -> MessageRow {
        let mut conn = pool.acquire().await.unwrap();
        find(&mut conn, id).await.unwrap().unwrap()
    }

    #[tokio::test]
    async fn finish_thinking_replaces_the_text_without_marking_it_edited() {
        let (pool, _dir) = test_pool().await;
        seed(&pool).await;
        thinking(&pool, 1).await;

        let mut conn = pool.acquire().await.unwrap();
        assert!(
            finish_thinking(&mut conn, 1, "You rolled 4", ANSWERED)
                .await
                .unwrap()
        );
        drop(conn);

        let message = row(&pool, 1).await;
        assert_eq!(message.content, "You rolled 4");
        assert_eq!(message.command_state, i64::from(ANSWERED));
        assert_eq!(message.edited_at, None);
        assert_eq!(message.interaction_user_id, Some(MEMBER));
    }

    #[tokio::test]
    async fn finish_thinking_returns_false_for_a_deleted_message() {
        let (pool, _dir) = test_pool().await;
        seed(&pool).await;
        thinking(&pool, 1).await;
        let mut conn = pool.acquire().await.unwrap();
        mark_deleted(&mut conn, 1).await.unwrap();

        assert!(
            !finish_thinking(&mut conn, 1, "You rolled 4", ANSWERED)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn finish_thinking_returns_false_when_already_finished() {
        let (pool, _dir) = test_pool().await;
        seed(&pool).await;
        thinking(&pool, 1).await;
        let mut conn = pool.acquire().await.unwrap();
        assert!(
            finish_thinking(&mut conn, 1, "first", ANSWERED)
                .await
                .unwrap()
        );

        assert!(
            !finish_thinking(&mut conn, 1, "second", ANSWERED)
                .await
                .unwrap()
        );
        drop(conn);
        assert_eq!(row(&pool, 1).await.content, "first");
    }

    #[tokio::test]
    async fn sweep_thinking_turns_every_thinking_message_into_no_answer() {
        let (pool, _dir) = test_pool().await;
        seed(&pool).await;
        thinking(&pool, 1).await;
        thinking(&pool, 2).await;
        let mut conn = pool.acquire().await.unwrap();
        let answered = CommandRun {
            name: "roll",
            user_id: MEMBER,
            state: ANSWERED,
        };
        insert_result(
            &mut conn,
            3,
            CHANNEL,
            PLUGIN_USER,
            "You rolled 4",
            Some(answered),
        )
        .await
        .unwrap();

        assert_eq!(sweep_thinking(&mut conn).await.unwrap(), 2);
        drop(conn);

        for id in [1, 2] {
            let message = row(&pool, id).await;
            assert_eq!(message.content, "Dice Bot didn't answer");
            assert_eq!(message.command_state, i64::from(NO_ANSWER));
        }
        let untouched = row(&pool, 3).await;
        assert_eq!(untouched.content, "You rolled 4");
        assert_eq!(untouched.command_state, i64::from(ANSWERED));
    }
}
