/// Draws an Opencord plugin's view from the core's patches (design §16):
/// the patch applier, the blocks, and the host the app gives them.
library;

export 'src/controller.dart'
    show
        BlocksController,
        BlocksRoots,
        BlocksModalRoot,
        BlocksDialog,
        BlocksError,
        BlocksEffect,
        CopyEffect,
        OpenLinkEffect,
        OpenChannelEffect,
        SoundEffect;
export 'src/host.dart';
export 'src/node.dart';
export 'src/theme.dart';
export 'src/view.dart' show BlocksView;
