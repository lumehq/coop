use common::profile::NostrProfile;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use gpui::SharedString;
use nostr_sdk::prelude::*;
use ui::text::render_plain_text_mut;

fn create_test_profiles() -> Vec<NostrProfile> {
    let mut profiles = Vec::new();

    // Create a few test profiles
    for i in 0..5 {
        let keypair = Keys::generate();
        let profile = NostrProfile {
            public_key: keypair.public_key(),
            name: SharedString::from(format!("user{}", i)),
            avatar: SharedString::from(format!("avatar{}", i)),
            // Add other required fields based on NostrProfile definition
            // This is a simplified version - adjust based on your actual NostrProfile struct
        };
        profiles.push(profile);
    }

    profiles
}

fn benchmark_plain_text(c: &mut Criterion) {
    let profiles = create_test_profiles();

    // Simple text without any links or entities
    let simple_text = "This is a simple text message without any links or entities.";

    // Text with URLs
    let text_with_urls =
        "Check out https://example.com and https://nostr.com for more information.";

    // Text with nostr entities
    let text_with_nostr = "I found this note nostr:note1qw5uy7hsqs4jcsvmjc2rj5t6f5uuenwg3yapm5l58srprspvshlspr4mh3 from npub1l2vyh47mk2p0qlsku7hg0vn29faehy9hy34ygaclpn66ukqp3afqutajft";

    // Mixed content with urls and nostr entities
    let mixed_content = "Check out https://example.com and my profile nostr:npub1l2vyh47mk2p0qlsku7hg0vn29faehy9hy34ygaclpn66ukqp3afqutajft along with this event nevent1qw5uy7hsqs4jcsvmjc2rj5t6f5uuenwg3yapm5l58srprspvshlspr4mh3";

    // Long text with multiple links and entities
    let long_text = "Here's a long message with multiple links like https://example1.com, https://example2.com, and https://example3.com. It also has nostr entities like npub1l2vyh47mk2p0qlsku7hg0vn29faehy9hy34ygaclpn66ukqp3afqutajft, note1qw5uy7hsqs4jcsvmjc2rj5t6f5uuenwg3yapm5l58srprspvshlspr4mh3, and nprofile1qqsrhuxx8l9ex335q7he0f09aej04zpazpl0ne2cgukyawd24mayt8gpp4mhxue69uhhytnc9e3k7mgpz4mhxue69uhkg6nzv9ejuerpd46hxtnfdupzp8xummjw3exgcnqvmpw35xjueqvdnyqystngfxk5hsnfd9h8jtr8a4klacnp".repeat(3);

    // Benchmark with simple text
    c.bench_function("render_plain_text_simple", |b| {
        b.iter(|| {
            let mut text = String::new();
            let mut highlights = Vec::new();
            let mut link_ranges = Vec::new();
            let mut link_urls = Vec::new();

            render_plain_text_mut(
                black_box(simple_text),
                black_box(&profiles),
                &mut text,
                &mut highlights,
                &mut link_ranges,
                &mut link_urls,
            )
        })
    });

    // Benchmark with URLs
    c.bench_function("render_plain_text_urls", |b| {
        b.iter(|| {
            let mut text = String::new();
            let mut highlights = Vec::new();
            let mut link_ranges = Vec::new();
            let mut link_urls = Vec::new();

            render_plain_text_mut(
                black_box(text_with_urls),
                black_box(&profiles),
                &mut text,
                &mut highlights,
                &mut link_ranges,
                &mut link_urls,
            )
        })
    });

    // Benchmark with nostr entities
    c.bench_function("render_plain_text_nostr", |b| {
        b.iter(|| {
            let mut text = String::new();
            let mut highlights = Vec::new();
            let mut link_ranges = Vec::new();
            let mut link_urls = Vec::new();

            render_plain_text_mut(
                black_box(text_with_nostr),
                black_box(&profiles),
                &mut text,
                &mut highlights,
                &mut link_ranges,
                &mut link_urls,
            )
        })
    });

    // Benchmark with mixed content
    c.bench_function("render_plain_text_mixed", |b| {
        b.iter(|| {
            let mut text = String::new();
            let mut highlights = Vec::new();
            let mut link_ranges = Vec::new();
            let mut link_urls = Vec::new();

            render_plain_text_mut(
                black_box(mixed_content),
                black_box(&profiles),
                &mut text,
                &mut highlights,
                &mut link_ranges,
                &mut link_urls,
            )
        })
    });

    // Benchmark with long text
    c.bench_function("render_plain_text_long", |b| {
        b.iter(|| {
            let mut text = String::new();
            let mut highlights = Vec::new();
            let mut link_ranges = Vec::new();
            let mut link_urls = Vec::new();

            render_plain_text_mut(
                black_box(&long_text),
                black_box(&profiles),
                &mut text,
                &mut highlights,
                &mut link_ranges,
                &mut link_urls,
            )
        })
    });
}

criterion_group!(benches, benchmark_plain_text);
criterion_main!(benches);
