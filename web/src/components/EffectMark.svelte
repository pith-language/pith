<script lang="ts">
    export type EffectCategory = {
        name: string;
        blurb: string;
        status: string;
        running: boolean;
    };

    let { categories }: { categories: EffectCategory[] } = $props();

    let active: number | null = $state(null);
    let scrollMode = $state(false);
    let hoverable = $state(false);
    let scroller = $state<HTMLDivElement | undefined>(undefined);

    $effect(() => {
        const narrow = window.matchMedia("(max-width: 760px)");
        const fine = window.matchMedia("(hover: hover) and (pointer: fine)");
        const apply = () => {
            scrollMode = narrow.matches;
            hoverable = fine.matches;
        };
        apply();
        narrow.addEventListener("change", apply);
        fine.addEventListener("change", apply);
        return () => {
            narrow.removeEventListener("change", apply);
            fine.removeEventListener("change", apply);
        };
    });

    $effect(() => {
        if (scrollMode === false || scroller === undefined) return;
        let frame = 0;
        const update = () => {
            frame = 0;
            const rect = scroller.getBoundingClientRect();
            const span = rect.height - window.innerHeight;
            const progress =
                span > 0 ? Math.min(1, Math.max(0, -rect.top / span)) : 0;
            active = Math.min(
                categories.length - 1,
                Math.floor(progress * categories.length),
            );
        };
        const onScroll = () => {
            if (!frame) frame = requestAnimationFrame(update);
        };
        update();
        window.addEventListener("scroll", onScroll, { passive: true });
        window.addEventListener("resize", onScroll, { passive: true });
        return () => {
            window.removeEventListener("scroll", onScroll);
            window.removeEventListener("resize", onScroll);
            if (frame) cancelAnimationFrame(frame);
        };
    });

    const petal = "M50,9 C56,19 56,29 50,39 C44,29 44,19 50,9 Z";
    const accentIndex = 3;

    const focus = (index: number) => () => (active = index);
    const release = () => (active = null);
</script>

<div class="scroller" bind:this={scroller}>
    <div class="effects">
        <div class="markwrap">
            <svg
                viewBox="0 0 100 100"
                class="mark"
                role="img"
                aria-label="The pith mark: one petal for each of the five effect categories around one kernel"
            >
                {#each categories as _category, i (i)}
                    <path
                        d={petal}
                        transform={`rotate(${i * 72} 50 50)`}
                        class:accent={i === accentIndex}
                        class:recede={active !== null && active !== i}
                    ></path>
                {/each}
                <circle cx="50" cy="50" r="4.2" class="kernel"></circle>
            </svg>
        </div>

        <ul class="cats">
            {#each categories as category, i (i)}
                <li class:on={active === i}>
                    <button
                        type="button"
                        inert={scrollMode || undefined}
                        onpointerenter={hoverable ? focus(i) : undefined}
                        onpointerleave={hoverable ? release : undefined}
                        onfocus={focus(i)}
                        onblur={release}
                    >
                        <span class="idx machine" aria-hidden="true">
                            0{i + 1} / 0{categories.length}
                        </span>
                        <span class="name label">{category.name}</span>
                        <span class="blurb">{category.blurb}</span>
                        <span
                            class="status machine"
                            class:running={category.running}
                        >
                            {category.status}
                        </span>
                    </button>
                </li>
            {/each}
        </ul>
    </div>
</div>

<style>
    .scroller {
        display: contents;
    }

    .effects {
        display: grid;
        grid-template-columns: minmax(200px, 300px) 1fr;
        gap: 32px 56px;
        align-items: center;
        color: var(--ink);
    }

    .mark {
        width: 100%;
        max-width: 300px;
    }

    .markwrap {
        display: flex;
        justify-content: center;
    }

    .mark path {
        fill: currentColor;
        transition:
            opacity 240ms cubic-bezier(0.25, 0.7, 0.25, 1),
            fill 240ms cubic-bezier(0.25, 0.7, 0.25, 1);
    }

    .mark path.recede {
        opacity: 0.15;
    }

    .mark path.accent {
        fill: var(--accent);
    }

    .mark .kernel {
        fill: var(--pith);
    }

    .cats li {
        border-bottom: 1px solid var(--line);
        border-left: 3px solid transparent;
        transition: border-color 240ms cubic-bezier(0.25, 0.7, 0.25, 1);
    }

    .cats li:first-child {
        border-top: 1px solid var(--line);
    }

    .cats li.on {
        border-left-color: var(--pith);
    }

    .cats button {
        display: grid;
        grid-template-columns: 128px 1fr auto;
        gap: 6px 20px;
        align-items: baseline;
        width: 100%;
        padding: 13px 2px 13px 14px;
        border: none;
        background: none;
        color: inherit;
        font: inherit;
        text-align: left;
        cursor: default;
    }

    .cats li :global(.label) {
        letter-spacing: 0.18em;
    }

    .cats .idx {
        display: none;
    }

    .cats .name {
        transition: color 240ms cubic-bezier(0.25, 0.7, 0.25, 1);
    }

    .cats .blurb {
        font-size: 16.5px;
        line-height: 1.5;
        color: var(--sub);
        transition: color 240ms cubic-bezier(0.25, 0.7, 0.25, 1);
    }

    .cats .status {
        color: var(--sub);
        white-space: nowrap;
        transition: color 240ms cubic-bezier(0.25, 0.7, 0.25, 1);
    }

    .cats .status.running {
        color: var(--pith);
    }

    .cats li.on :global(.label),
    .cats li.on .blurb {
        color: var(--ink);
    }

    @media (max-width: 760px) {
        .scroller {
            display: block;
            height: 500vh;
        }

        .effects {
            position: sticky;
            top: 0;
            height: 100vh;
            display: flex;
            flex-direction: column;
            background: var(--ground);
        }

        .markwrap {
            padding: 22px 0 18px;
            border-bottom: 1.5px solid var(--line);
        }

        .mark {
            max-width: 160px;
        }

        .cats {
            position: relative;
            flex: 1;
            pointer-events: none;
        }

        .cats li {
            position: absolute;
            inset: 0;
            display: flex;
            align-items: center;
            border: none;
            opacity: 0;
            transform: translateY(12px);
            transition:
                opacity 320ms cubic-bezier(0.25, 0.7, 0.25, 1),
                transform 320ms cubic-bezier(0.25, 0.7, 0.25, 1);
        }

        .cats li.on {
            opacity: 1;
            transform: none;
        }

        .cats button {
            grid-template-columns: 1fr;
            gap: 18px;
            padding: 28px 2px 28px 12px;
        }

        .cats .idx {
            display: block;
            color: var(--sub);
        }

        .cats .name {
            font-size: 14px;
            letter-spacing: 0.24em;
        }

        .cats .blurb {
            font-size: 19px;
            line-height: 1.6;
            max-width: 26ch;
        }

        .cats .status {
            white-space: normal;
        }
    }

    @supports (height: 1dvh) {
        @media (max-width: 760px) {
            .scroller {
                height: 500dvh;
            }

            .effects {
                height: 100dvh;
            }
        }
    }
</style>
