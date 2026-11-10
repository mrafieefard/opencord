import 'package:flutter/widgets.dart';

import '../common_props.dart';
import '../view.dart';

/// `image` (design §12.2, §13.1): loads images from assets, storage, URL,
/// attachments or bytes through [BlocksHost.image].
final imageBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
  return _ImageBlockWidget(key: ValueKey<int>(b.node.id), build: b);
}

class _ImageBlockWidget extends StatefulWidget {
  const _ImageBlockWidget({super.key, required this.build});

  final BlockBuild build;

  @override
  State<_ImageBlockWidget> createState() => _ImageBlockWidgetState();
}

class _ImageBlockWidgetState extends State<_ImageBlockWidget> {
  ImageProvider? _provider;
  ImageStream? _stream;
  ImageStreamListener? _listener;
  bool _loading = true;
  bool _hasError = false;
  Map<String, Object?>? _source;
  Object? _version;

  @override
  void initState() {
    super.initState();
    _source = imageSource(widget.build.props);
    _version = widget.build.props['version'];
    _load();
  }

  @override
  void didUpdateWidget(_ImageBlockWidget oldWidget) {
    super.didUpdateWidget(oldWidget);
    final nextSource = imageSource(widget.build.props);
    final nextVersion = widget.build.props['version'];
    if (!_mapsEqual(_source, nextSource) || _version != nextVersion) {
      _source = nextSource;
      _version = nextVersion;
      _load();
    }
  }

  @override
  void dispose() {
    _cleanStream();
    super.dispose();
  }

  void _cleanStream() {
    if (_stream != null && _listener != null) {
      _stream!.removeListener(_listener!);
    }
    _stream = null;
    _listener = null;
  }

  bool _mapsEqual(Map<String, Object?>? a, Map<String, Object?>? b) {
    if (identical(a, b)) return true;
    if (a == null || b == null || a.length != b.length) return false;
    for (final key in a.keys) {
      if (a[key] != b[key]) return false;
    }
    return true;
  }

  void _load() {
    _cleanStream();
    final source = _source;
    if (source == null) {
      setState(() {
        _loading = false;
        _hasError = true;
      });
      return;
    }

    setState(() {
      _loading = true;
      _hasError = false;
    });

    widget.build.host
        .image(source)
        .then<void>(
          (provider) {
            if (!mounted) return;
            if (provider == null) {
              setState(() {
                _loading = false;
                _hasError = true;
              });
              if (widget.build.node.handles('error')) {
                widget.build.send('error', {'message': 'Image not found'});
              }
              return;
            }

            _provider = provider;
            final stream = provider.resolve(ImageConfiguration.empty);
            _stream = stream;
            final listener = ImageStreamListener(
              (info, synchronousCall) {
                if (!mounted) return;
                setState(() {
                  _loading = false;
                });
                if (widget.build.node.handles('load')) {
                  widget.build.send('load', {
                    'width': info.image.width.toDouble(),
                    'height': info.image.height.toDouble(),
                  });
                }
              },
              onError: (exception, stackTrace) {
                if (!mounted) return;
                setState(() {
                  _loading = false;
                  _hasError = true;
                });
                if (widget.build.node.handles('error')) {
                  widget.build.send('error', {'message': exception.toString()});
                }
              },
            );
            _listener = listener;
            stream.addListener(listener);
          },
          onError: (Object error) {
            if (!mounted) return;
            setState(() {
              _loading = false;
              _hasError = true;
            });
            if (widget.build.node.handles('error')) {
              widget.build.send('error', {'message': error.toString()});
            }
          },
        );
  }

  @override
  Widget build(BuildContext context) {
    final props = widget.build.props;
    final radius = numberProp(props['radius']) ?? 0;
    final placeholderColor = widget.build.theme.resolveColor(
      props['placeholder'],
    );
    final fit = _boxFit(props['fit'] as String?);

    Widget content;
    if (_loading) {
      content = placeholderColor != null
          ? ColoredBox(color: placeholderColor)
          : const SizedBox.shrink();
    } else if (_hasError || _provider == null) {
      content = placeholderColor != null
          ? ColoredBox(color: placeholderColor)
          : const SizedBox.shrink();
    } else {
      content = Image(image: _provider!, fit: fit);
    }

    if (radius > 0) {
      content = ClipRRect(
        borderRadius: BorderRadius.circular(radius),
        child: content,
      );
    }

    return content;
  }
}

BoxFit _boxFit(String? fit) => switch (fit) {
  'cover' => BoxFit.cover,
  'fill' => BoxFit.fill,
  'none' => BoxFit.none,
  _ => BoxFit.contain,
};
