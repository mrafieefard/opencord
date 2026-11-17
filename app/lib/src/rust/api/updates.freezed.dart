// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'updates.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$UpdateEvent {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UpdateEvent);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'UpdateEvent()';
}


}

/// @nodoc
class $UpdateEventCopyWith<$Res>  {
$UpdateEventCopyWith(UpdateEvent _, $Res Function(UpdateEvent) __);
}


/// Adds pattern-matching-related methods to [UpdateEvent].
extension UpdateEventPatterns on UpdateEvent {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( UpdateEvent_Downloaded value)?  downloaded,TResult Function( UpdateEvent_AlreadyWaiting value)?  alreadyWaiting,TResult Function( UpdateEvent_Failed value)?  failed,required TResult orElse(),}){
final _that = this;
switch (_that) {
case UpdateEvent_Downloaded() when downloaded != null:
return downloaded(_that);case UpdateEvent_AlreadyWaiting() when alreadyWaiting != null:
return alreadyWaiting(_that);case UpdateEvent_Failed() when failed != null:
return failed(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( UpdateEvent_Downloaded value)  downloaded,required TResult Function( UpdateEvent_AlreadyWaiting value)  alreadyWaiting,required TResult Function( UpdateEvent_Failed value)  failed,}){
final _that = this;
switch (_that) {
case UpdateEvent_Downloaded():
return downloaded(_that);case UpdateEvent_AlreadyWaiting():
return alreadyWaiting(_that);case UpdateEvent_Failed():
return failed(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( UpdateEvent_Downloaded value)?  downloaded,TResult? Function( UpdateEvent_AlreadyWaiting value)?  alreadyWaiting,TResult? Function( UpdateEvent_Failed value)?  failed,}){
final _that = this;
switch (_that) {
case UpdateEvent_Downloaded() when downloaded != null:
return downloaded(_that);case UpdateEvent_AlreadyWaiting() when alreadyWaiting != null:
return alreadyWaiting(_that);case UpdateEvent_Failed() when failed != null:
return failed(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String version,  String downloadJson)?  downloaded,TResult Function( String version)?  alreadyWaiting,TResult Function( String kind,  String message)?  failed,required TResult orElse(),}) {final _that = this;
switch (_that) {
case UpdateEvent_Downloaded() when downloaded != null:
return downloaded(_that.version,_that.downloadJson);case UpdateEvent_AlreadyWaiting() when alreadyWaiting != null:
return alreadyWaiting(_that.version);case UpdateEvent_Failed() when failed != null:
return failed(_that.kind,_that.message);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String version,  String downloadJson)  downloaded,required TResult Function( String version)  alreadyWaiting,required TResult Function( String kind,  String message)  failed,}) {final _that = this;
switch (_that) {
case UpdateEvent_Downloaded():
return downloaded(_that.version,_that.downloadJson);case UpdateEvent_AlreadyWaiting():
return alreadyWaiting(_that.version);case UpdateEvent_Failed():
return failed(_that.kind,_that.message);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String version,  String downloadJson)?  downloaded,TResult? Function( String version)?  alreadyWaiting,TResult? Function( String kind,  String message)?  failed,}) {final _that = this;
switch (_that) {
case UpdateEvent_Downloaded() when downloaded != null:
return downloaded(_that.version,_that.downloadJson);case UpdateEvent_AlreadyWaiting() when alreadyWaiting != null:
return alreadyWaiting(_that.version);case UpdateEvent_Failed() when failed != null:
return failed(_that.kind,_that.message);case _:
  return null;

}
}

}

/// @nodoc


class UpdateEvent_Downloaded extends UpdateEvent {
  const UpdateEvent_Downloaded({required this.version, required this.downloadJson}): super._();
  

 final  String version;
 final  String downloadJson;

/// Create a copy of UpdateEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UpdateEvent_DownloadedCopyWith<UpdateEvent_Downloaded> get copyWith => _$UpdateEvent_DownloadedCopyWithImpl<UpdateEvent_Downloaded>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UpdateEvent_Downloaded&&(identical(other.version, version) || other.version == version)&&(identical(other.downloadJson, downloadJson) || other.downloadJson == downloadJson));
}


@override
int get hashCode => Object.hash(runtimeType,version,downloadJson);

@override
String toString() {
  return 'UpdateEvent.downloaded(version: $version, downloadJson: $downloadJson)';
}


}

/// @nodoc
abstract mixin class $UpdateEvent_DownloadedCopyWith<$Res> implements $UpdateEventCopyWith<$Res> {
  factory $UpdateEvent_DownloadedCopyWith(UpdateEvent_Downloaded value, $Res Function(UpdateEvent_Downloaded) _then) = _$UpdateEvent_DownloadedCopyWithImpl;
@useResult
$Res call({
 String version, String downloadJson
});




}
/// @nodoc
class _$UpdateEvent_DownloadedCopyWithImpl<$Res>
    implements $UpdateEvent_DownloadedCopyWith<$Res> {
  _$UpdateEvent_DownloadedCopyWithImpl(this._self, this._then);

  final UpdateEvent_Downloaded _self;
  final $Res Function(UpdateEvent_Downloaded) _then;

/// Create a copy of UpdateEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? version = null,Object? downloadJson = null,}) {
  return _then(UpdateEvent_Downloaded(
version: null == version ? _self.version : version // ignore: cast_nullable_to_non_nullable
as String,downloadJson: null == downloadJson ? _self.downloadJson : downloadJson // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class UpdateEvent_AlreadyWaiting extends UpdateEvent {
  const UpdateEvent_AlreadyWaiting({required this.version}): super._();
  

 final  String version;

/// Create a copy of UpdateEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UpdateEvent_AlreadyWaitingCopyWith<UpdateEvent_AlreadyWaiting> get copyWith => _$UpdateEvent_AlreadyWaitingCopyWithImpl<UpdateEvent_AlreadyWaiting>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UpdateEvent_AlreadyWaiting&&(identical(other.version, version) || other.version == version));
}


@override
int get hashCode => Object.hash(runtimeType,version);

@override
String toString() {
  return 'UpdateEvent.alreadyWaiting(version: $version)';
}


}

/// @nodoc
abstract mixin class $UpdateEvent_AlreadyWaitingCopyWith<$Res> implements $UpdateEventCopyWith<$Res> {
  factory $UpdateEvent_AlreadyWaitingCopyWith(UpdateEvent_AlreadyWaiting value, $Res Function(UpdateEvent_AlreadyWaiting) _then) = _$UpdateEvent_AlreadyWaitingCopyWithImpl;
@useResult
$Res call({
 String version
});




}
/// @nodoc
class _$UpdateEvent_AlreadyWaitingCopyWithImpl<$Res>
    implements $UpdateEvent_AlreadyWaitingCopyWith<$Res> {
  _$UpdateEvent_AlreadyWaitingCopyWithImpl(this._self, this._then);

  final UpdateEvent_AlreadyWaiting _self;
  final $Res Function(UpdateEvent_AlreadyWaiting) _then;

/// Create a copy of UpdateEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? version = null,}) {
  return _then(UpdateEvent_AlreadyWaiting(
version: null == version ? _self.version : version // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class UpdateEvent_Failed extends UpdateEvent {
  const UpdateEvent_Failed({required this.kind, required this.message}): super._();
  

 final  String kind;
 final  String message;

/// Create a copy of UpdateEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UpdateEvent_FailedCopyWith<UpdateEvent_Failed> get copyWith => _$UpdateEvent_FailedCopyWithImpl<UpdateEvent_Failed>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UpdateEvent_Failed&&(identical(other.kind, kind) || other.kind == kind)&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode => Object.hash(runtimeType,kind,message);

@override
String toString() {
  return 'UpdateEvent.failed(kind: $kind, message: $message)';
}


}

/// @nodoc
abstract mixin class $UpdateEvent_FailedCopyWith<$Res> implements $UpdateEventCopyWith<$Res> {
  factory $UpdateEvent_FailedCopyWith(UpdateEvent_Failed value, $Res Function(UpdateEvent_Failed) _then) = _$UpdateEvent_FailedCopyWithImpl;
@useResult
$Res call({
 String kind, String message
});




}
/// @nodoc
class _$UpdateEvent_FailedCopyWithImpl<$Res>
    implements $UpdateEvent_FailedCopyWith<$Res> {
  _$UpdateEvent_FailedCopyWithImpl(this._self, this._then);

  final UpdateEvent_Failed _self;
  final $Res Function(UpdateEvent_Failed) _then;

/// Create a copy of UpdateEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? kind = null,Object? message = null,}) {
  return _then(UpdateEvent_Failed(
kind: null == kind ? _self.kind : kind // ignore: cast_nullable_to_non_nullable
as String,message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
