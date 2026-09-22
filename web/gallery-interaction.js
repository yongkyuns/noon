function pointerSelectionMaxMovement(example) {
  const interaction = example?.interaction ?? null;
  return interaction?.type === "pointer-fill-selection" ? interaction.maxMovement : null;
}

// Tracks only host-side configuration that Noon intentionally does not replay
// across semantic-session replacement. Scene interaction semantics remain in
// Rust; this controller only avoids issuing redundant/default configuration
// calls into callback-driven scenes.
export class GalleryInteractionController {
  #player = null;
  #pointerSelectionEnabled = false;

  async beforeReconcile(player) {
    if (player == null) return;
    this.#adoptPlayer(player);
    if (!this.#pointerSelectionEnabled) return;
    await player.setPointerFillSelection(null);
    this.#pointerSelectionEnabled = false;
  }

  resetForSession(player = null) {
    this.#player = player;
    this.#pointerSelectionEnabled = false;
  }

  async apply(player, example) {
    if (player == null) return;
    // A newly adopted/reconciled semantic session starts with interaction
    // configuration disabled; configuration is deliberately not replayed.
    this.resetForSession(player);
    const maxMovement = pointerSelectionMaxMovement(example);
    if (maxMovement === null) return;
    await player.setPointerFillSelection(maxMovement);
    this.#pointerSelectionEnabled = true;
  }

  #adoptPlayer(player) {
    if (this.#player === player) return;
    this.#player = player;
    this.#pointerSelectionEnabled = false;
  }
}
