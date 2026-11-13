//! Answered `ask human` questions are listed for the ticket's Messages section ("Logged in SHOP-21 Messages", *10 →
//! Answer · inline*); open ones are not.

use otaq_core::service::testing::TestService;

#[tokio::test(flavor = "multi_thread")]
async fn answered_questions_are_listed_oldest_first() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Order confirmation email").await;
    let first = w
        .board
        .store_question(
            &t.id,
            "Which email provider?",
            &["Postmark".into(), "SES".into()],
            &[],
            &[1],
            false,
        )
        .await
        .unwrap();
    w.board
        .record_answer(&first.id, "Answer to your question: Postmark (option 1).")
        .await
        .unwrap();
    w.board
        .store_question(&t.id, "Still open?", &[], &[], &[], false)
        .await
        .unwrap();
    let answered = w.board.answered_human_questions(&t.id).await.unwrap();
    assert_eq!(answered.len(), 1);
    assert_eq!(answered[0].question, "Which email provider?");
    assert_eq!(
        answered[0].answer,
        "Answer to your question: Postmark (option 1)."
    );
    assert!(answered[0].answered_at >= answered[0].asked_at);
    w.stop().await;
}

/// The checker and every later stage read what the user answered: `prime` and the checker's prime list each
/// answered question with its answer; an open one is not there yet.
#[tokio::test(flavor = "multi_thread")]
async fn later_stages_and_the_checker_read_the_users_answers() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Change the greeting").await;
    let asked = w
        .board
        .store_question(
            &t.id,
            "Which greeting should it print?",
            &[],
            &[],
            &[],
            false,
        )
        .await
        .unwrap();
    w.board
        .record_answer(&asked.id, "Hello from Otaq")
        .await
        .unwrap();
    w.board
        .store_question(&t.id, "Anything else?", &[], &[], &[], false)
        .await
        .unwrap();
    let key = t.key.to_string();
    let working = w
        .board
        .prime_context(&key, w.adapters.for_new_session())
        .await
        .unwrap();
    let checker = w.board.validate_prime(&key).await.unwrap();
    for text in [
        otaq_core::prime::render(&working),
        otaq_core::prime::render_validate(&checker),
    ] {
        assert!(text.contains("## The user's answers"), "{text}");
        assert!(
            text.contains(
                "- Asked:\n  > Which greeting should it print?\n  Answered:\n  > Hello from Otaq\n"
            ),
            "{text}"
        );
        assert!(!text.contains("Anything else?"), "{text}");
    }
    w.stop().await;
}
