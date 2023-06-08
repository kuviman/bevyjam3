---
---

# I used Bevy for the first time for a game jam

And we won

![img](../Lpko5O-transparent.png)

[PLAY THE GAME](https://kuviman.itch.io/linksider)

[Source code](https://github.com/kuviman/linksider)

## Intro

I have [participated in a bunch of game jams using Rust](https://kuviman.itch.io).

Usually I am using [my own engine](https://github.com/geng-engine/geng). But I wanted to give [bevy](https://bevyengine.org/) a shot for a while since I was interested in trying out the ECS approach.
I was working on my game and was not very happy about the code quality so I was looking into how to organize my code better.

I have even been telling people to use bevy instead of my engine in previous posts, despite never actually trying it.

So participating in the [Bevy Jam 3](https://itch.io/jam/bevy-jam-3) was a great opportunity for me.

*The goal of this post is mostly to document my experience with bevy for myself. The issues I encountered might be because of my impatience, or me failing to look for a solution and assuming things.*

## Day 1

On day 1 we were starting out with bevy for the first time ever, so we went ahead and read the bevy book. Surprisingly it was very short.

One of the first things to notice was very long compilation time.
Which wouldnt really matter too much for me since building of the dependencies is only supposed to happen once. But, almost every time I added a new dependency to the project (like `bevy_rapier` or `bevy_ecs_ldtk`) it was recompiling the entire world from scratch again, making me wait for 20 minutes which made me not want to add new dependencies ever even if I would benefit from them. It would be faster if I didn't use `--jobs 2` for compilation, but that would cause unpleasant lagging of the system.

Also dynamic linking was set up from beginning, like shown in the book.
But I was getting a lot of linker errors (about unresolved symbols) from time to time for some reason. Only full recompilation was solving that issue, so we ended up using regular static linking instead which made us wait for 20-40 secs for every small change in game code.

Using [trunk](https://trunkrs.dev/) was really nice, the only issue with it was making it work using relative urls for itch/github pages: [issue #395](https://github.com/thedodd/trunk/issues/395)

By the end of the day we had a moving crab 🦀 with a sound effect playing on a keypress as a test of what we can do with bevy. It seemed like we figured everything we needed to know in order to make a game

While I was busy figuring out bevy stuff, **Daivy** was busy coming up with the game idea and this is what he came up with:

![idea](game-idea.png)

Basically, you play as a cube and you can upgrade each of your side with side effects, like jump/slide/etc and you use those powers to do platforming.

## Physics

The idea was really nice on paper so we started implementing it on day 2. It looked like we needed to have physics in the game, so we added [bevy_rapier2d](https://crates.io/crates/bevy_rapier2d) and started figuring out how to use that.

The biggest problem I had with it was the discoverability issue.
Like, the question I had all the time was which components do I need for systems to actually work.
For example, we had sensors on all 4 sides of the cube, but they were not detecting collisions with the level for some reason. Eventually I figured we need to opt in for collision detection between static bodies, since updating sensor positions was made not using physics but by modifying their `Transform` component directly, rapier still assumed that the sensor was static.
I think this is solved in bevy usually by having the `Bundle` types which let you see what you actually need.
Also using [bevy-inspector-egui](https://crates.io/crates/bevy-inspector-egui) was found to be useful to see what is being spawned by foreign systems.

![physics](physics.gif)

## Ldtk

In order to make levels we decided to try out [LDTK](https://ldtk.io/).
We never used it before but heard good things about it, and there was an existing [plugin for bevy](https://crates.io/crates/bevy_ecs_ldtk).

It was very simple to load the level tilemap and have it drawn on the screen (although initially I had to realise that I should not put my camera at z=0).

For spawning the entities & int grid cells `bevy_ecs_ldtk` uses the derive macro approach which I was not really a big fan of.
I think it would be easier to use if instead of writing logic in derive attributes I could simple use any regular Rust function/closure returning a bundle, so like instead of:

```rs
app.register_ldtk_entity::<PlayerBundle>("Player")

#[derive(LdtkEntity)]
struct PlayerBundle {
    ..
}
```

I would appreciate an API like this:

```rs
let player_constructor: impl Fn(&LdtkEntityInstance) -> impl Bundle;
app.register_ldtk_entity("Player", player_constructor);
```

One of the things that didn't work well with physics was just spawning a box collider for every wall tile.
This made player hit point between two tiles, even though it is flat ground.

One of the examples involved combining tiles together if they form a line, but that was still not good enough, so we had to implement another way of spawning the colliders.

The way it worked was spawning a polyline for every tile, and then despawning a segment if it was present twice - which means that the segment is between two tiles.

One other thing I missed from `bevy_ecs_ldtk` was registering int cells by name instead of by actual int value, since there is an option to specify names in ldtk itself.

<!-- cheeseburge -->

## Rewrite from scratch

So, at some point, we had working level loading, working physics with jump and slide side effects. But the game was not fun to play, it was very hard to control your character. So we needed to think about how to change the gameplay.

We had a couple ideas, but the one we decided to try out was removing the physics aspect of the game completely and turning it into a "turn based" puzzle game instead.

So, 4 days before the jam deadline, we started rewriting the game from scratch.

The first day of the rewrite went really slow because I was struggling with understanding of how to represent the turn based logic in bevy.
What we ended up with was using bevy states, going in a loop like `Processing Turn -> Animation -> Processing Turn -> Animation -> ...`.
Also, processing turn could end up with not requiring any animation (if player is stable), which switched the state to `Waiting for player input` state instead.
I think the way we did it was very far from ideal, but it worked, so we kept it as is. But I think this is the most unreliable part of the game code, and if I touch it something will most likely break.

I feel like the `Turn` should not be a state since it is something that happens instantly.
I was thinking it should have been a custom schedule maybe, that I would run manually somehow,
but we had to continue with other things.

After we figured that part out, it seemed like the struggle with bevy finally stopped and the last 3 days of the jam were very productive.

## Polish

Basically 2 days before the deadline we had all of the gameplay implemented and reserved the remaining time to polish the game and level design.

To make sound effects we used [sfxr](https://sfxr.me).

The entire process of development was streamed on twitch, and I was asking everyone if they wanted to help us with the game, and
**shadow_crushers** helped with visual effects and made us a nice music track:

{{ audio(src="game_music.ogg") }}

And later the music was covered by **Brainoid**, which we decided to keep:

{{ audio(src="KuviBevy.ogg") }}

One of the last things that was added was the background image. I was looking for a way to draw tiled background in bevy for quite some time, all I found was the [bevy_tiling_background](https://crates.io/crates/bevy_tiling_background) crate, but it was tied to screen coordinates and did not work on the web.
So instead I just spawned 9 regular sprites for that.

## Some other issues I had with bevy

I don't like the way of handling the assets.
Treating everything like it might not be loaded yet feels like we are back in the null world.

When playing audio, in order to control the audio effect, you need to convert your `Handle<AudioSink>` into a strong `Handle<AudioSink>`, but the type is exactly the same which is very weird imo. Also, when you just start playing the audio using `audio.play(source)` you get the handle to the sink that is not created yet and you can not control the audio immediately even if you have the strong handle.

When playing other bevy jam games I have seen that pretty much every single one is suffering from audio glitches on the web builds. As I understand, it is because audio processing is happening using Rust code instead of through actual web audio APIs, since wasm is single threaded.

I tried figuring out how to load configuration from lets say a json file at runtime, but could not find a way.
Otherwise a bunch of constants would be hardcoded in rust files, which is also not a good thing imo.
Using `std::fs` would not make it work on the web, so `include_str!` is the most ergonomic thing I found,
although we ended up not needing such a file, since all configuration we needed was inside ldtk.

One of the bugs when we still had physics was that if in one frame two sides were colliding with powerup, both could get the upgrade.
What I wanted to do was transferring the `Jump` component from the powerup entity to the player side entity, but I have no idea how to take ownership of a component.
All `Commands` let me do is `remove::<Jump>()` and `insert(Jump)`, meaning that I constructed component again instead of moving it, which made it very easy to miss that bug.

## Results

Here's the development timelapse:

{{ youtube(id="hwCZccbe4DI", autoplay=1) }}

This is 500x sped up video, which means that I spent 66.6 hours making this game.
And, of course, I did not make this game alone.

We ended up with a really good game that we enjoyed ourselves a lot, and we are now thinking about continuing working on it.
Taking #1 place on the jam is also nice and we thank the community for high ratings.

![gg](gg.gif)

## Would I recommend bevy?

Yes. I would recommend bevy to people who want to learn Rust by creating a game.

It does not require a deep understanding of lifetimes etc and maybe you don't need to fight the borrow checker too much, so especially if you are coming from a different language I feel like bevy is a good choice.

## Will I use bevy again?

At this point I don't think I will use it for new projects. The reason is I feel like the bevy ecs architecture, while letting me to split my code easily into different systems, moves a lot of checks from compile to runtime, which feels like I am giving up on Rust features that make it such a great language.

Instead I will try do despaghettify my code in some other way.
I still can rely on my code and I have this feeling of
"if it compiles it runs" which I don't have when using bevy.

I feel like there should be something without the tradeoff available for Rust, but it has not been discovered yet.

That said, we are thinking about working more on this game,
and I suppose it makes sense to continue working in bevy without rewriting it, so maybe we will learn and appreciate that approach more with time.

Here's some stuff that I find interesting from other people:

- <https://molentum.me/blog/starframe-architecture/>
- <https://github.com/kvark/froggy>
- <https://www.anthropicstudios.com/2019/06/05/entity-systems/>

And we also started an experiment of our own recently, but its too early to talk about it.

Anyway, jamming is always fun and this jam was no exception. Congratulations to all who participated and made a game!
