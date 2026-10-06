// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'types.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$AddServerOutcome {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AddServerOutcome);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'AddServerOutcome()';
}


}

/// @nodoc
class $AddServerOutcomeCopyWith<$Res>  {
$AddServerOutcomeCopyWith(AddServerOutcome _, $Res Function(AddServerOutcome) __);
}


/// Adds pattern-matching-related methods to [AddServerOutcome].
extension AddServerOutcomePatterns on AddServerOutcome {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( AddServerOutcome_Added value)?  added,TResult Function( AddServerOutcome_NeedsTrust value)?  needsTrust,required TResult orElse(),}){
final _that = this;
switch (_that) {
case AddServerOutcome_Added() when added != null:
return added(_that);case AddServerOutcome_NeedsTrust() when needsTrust != null:
return needsTrust(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( AddServerOutcome_Added value)  added,required TResult Function( AddServerOutcome_NeedsTrust value)  needsTrust,}){
final _that = this;
switch (_that) {
case AddServerOutcome_Added():
return added(_that);case AddServerOutcome_NeedsTrust():
return needsTrust(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( AddServerOutcome_Added value)?  added,TResult? Function( AddServerOutcome_NeedsTrust value)?  needsTrust,}){
final _that = this;
switch (_that) {
case AddServerOutcome_Added() when added != null:
return added(_that);case AddServerOutcome_NeedsTrust() when needsTrust != null:
return needsTrust(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( Server field0)?  added,TResult Function( String address,  String fingerprint)?  needsTrust,required TResult orElse(),}) {final _that = this;
switch (_that) {
case AddServerOutcome_Added() when added != null:
return added(_that.field0);case AddServerOutcome_NeedsTrust() when needsTrust != null:
return needsTrust(_that.address,_that.fingerprint);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( Server field0)  added,required TResult Function( String address,  String fingerprint)  needsTrust,}) {final _that = this;
switch (_that) {
case AddServerOutcome_Added():
return added(_that.field0);case AddServerOutcome_NeedsTrust():
return needsTrust(_that.address,_that.fingerprint);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( Server field0)?  added,TResult? Function( String address,  String fingerprint)?  needsTrust,}) {final _that = this;
switch (_that) {
case AddServerOutcome_Added() when added != null:
return added(_that.field0);case AddServerOutcome_NeedsTrust() when needsTrust != null:
return needsTrust(_that.address,_that.fingerprint);case _:
  return null;

}
}

}

/// @nodoc


class AddServerOutcome_Added extends AddServerOutcome {
  const AddServerOutcome_Added(this.field0): super._();
  

 final  Server field0;

/// Create a copy of AddServerOutcome
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AddServerOutcome_AddedCopyWith<AddServerOutcome_Added> get copyWith => _$AddServerOutcome_AddedCopyWithImpl<AddServerOutcome_Added>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AddServerOutcome_Added&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'AddServerOutcome.added(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $AddServerOutcome_AddedCopyWith<$Res> implements $AddServerOutcomeCopyWith<$Res> {
  factory $AddServerOutcome_AddedCopyWith(AddServerOutcome_Added value, $Res Function(AddServerOutcome_Added) _then) = _$AddServerOutcome_AddedCopyWithImpl;
@useResult
$Res call({
 Server field0
});




}
/// @nodoc
class _$AddServerOutcome_AddedCopyWithImpl<$Res>
    implements $AddServerOutcome_AddedCopyWith<$Res> {
  _$AddServerOutcome_AddedCopyWithImpl(this._self, this._then);

  final AddServerOutcome_Added _self;
  final $Res Function(AddServerOutcome_Added) _then;

/// Create a copy of AddServerOutcome
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(AddServerOutcome_Added(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as Server,
  ));
}


}

/// @nodoc


class AddServerOutcome_NeedsTrust extends AddServerOutcome {
  const AddServerOutcome_NeedsTrust({required this.address, required this.fingerprint}): super._();
  

 final  String address;
 final  String fingerprint;

/// Create a copy of AddServerOutcome
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$AddServerOutcome_NeedsTrustCopyWith<AddServerOutcome_NeedsTrust> get copyWith => _$AddServerOutcome_NeedsTrustCopyWithImpl<AddServerOutcome_NeedsTrust>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is AddServerOutcome_NeedsTrust&&(identical(other.address, address) || other.address == address)&&(identical(other.fingerprint, fingerprint) || other.fingerprint == fingerprint));
}


@override
int get hashCode => Object.hash(runtimeType,address,fingerprint);

@override
String toString() {
  return 'AddServerOutcome.needsTrust(address: $address, fingerprint: $fingerprint)';
}


}

/// @nodoc
abstract mixin class $AddServerOutcome_NeedsTrustCopyWith<$Res> implements $AddServerOutcomeCopyWith<$Res> {
  factory $AddServerOutcome_NeedsTrustCopyWith(AddServerOutcome_NeedsTrust value, $Res Function(AddServerOutcome_NeedsTrust) _then) = _$AddServerOutcome_NeedsTrustCopyWithImpl;
@useResult
$Res call({
 String address, String fingerprint
});




}
/// @nodoc
class _$AddServerOutcome_NeedsTrustCopyWithImpl<$Res>
    implements $AddServerOutcome_NeedsTrustCopyWith<$Res> {
  _$AddServerOutcome_NeedsTrustCopyWithImpl(this._self, this._then);

  final AddServerOutcome_NeedsTrust _self;
  final $Res Function(AddServerOutcome_NeedsTrust) _then;

/// Create a copy of AddServerOutcome
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? address = null,Object? fingerprint = null,}) {
  return _then(AddServerOutcome_NeedsTrust(
address: null == address ? _self.address : address // ignore: cast_nullable_to_non_nullable
as String,fingerprint: null == fingerprint ? _self.fingerprint : fingerprint // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc
mixin _$ConnectionState {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ConnectionState);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'ConnectionState()';
}


}

/// @nodoc
class $ConnectionStateCopyWith<$Res>  {
$ConnectionStateCopyWith(ConnectionState _, $Res Function(ConnectionState) __);
}


/// Adds pattern-matching-related methods to [ConnectionState].
extension ConnectionStatePatterns on ConnectionState {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( ConnectionState_Connecting value)?  connecting,TResult Function( ConnectionState_Connected value)?  connected,TResult Function( ConnectionState_Reconnecting value)?  reconnecting,TResult Function( ConnectionState_Failed value)?  failed,required TResult orElse(),}){
final _that = this;
switch (_that) {
case ConnectionState_Connecting() when connecting != null:
return connecting(_that);case ConnectionState_Connected() when connected != null:
return connected(_that);case ConnectionState_Reconnecting() when reconnecting != null:
return reconnecting(_that);case ConnectionState_Failed() when failed != null:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( ConnectionState_Connecting value)  connecting,required TResult Function( ConnectionState_Connected value)  connected,required TResult Function( ConnectionState_Reconnecting value)  reconnecting,required TResult Function( ConnectionState_Failed value)  failed,}){
final _that = this;
switch (_that) {
case ConnectionState_Connecting():
return connecting(_that);case ConnectionState_Connected():
return connected(_that);case ConnectionState_Reconnecting():
return reconnecting(_that);case ConnectionState_Failed():
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( ConnectionState_Connecting value)?  connecting,TResult? Function( ConnectionState_Connected value)?  connected,TResult? Function( ConnectionState_Reconnecting value)?  reconnecting,TResult? Function( ConnectionState_Failed value)?  failed,}){
final _that = this;
switch (_that) {
case ConnectionState_Connecting() when connecting != null:
return connecting(_that);case ConnectionState_Connected() when connected != null:
return connected(_that);case ConnectionState_Reconnecting() when reconnecting != null:
return reconnecting(_that);case ConnectionState_Failed() when failed != null:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function()?  connecting,TResult Function()?  connected,TResult Function( int attempt,  int retryInMs)?  reconnecting,TResult Function( FailureReason reason,  String message)?  failed,required TResult orElse(),}) {final _that = this;
switch (_that) {
case ConnectionState_Connecting() when connecting != null:
return connecting();case ConnectionState_Connected() when connected != null:
return connected();case ConnectionState_Reconnecting() when reconnecting != null:
return reconnecting(_that.attempt,_that.retryInMs);case ConnectionState_Failed() when failed != null:
return failed(_that.reason,_that.message);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function()  connecting,required TResult Function()  connected,required TResult Function( int attempt,  int retryInMs)  reconnecting,required TResult Function( FailureReason reason,  String message)  failed,}) {final _that = this;
switch (_that) {
case ConnectionState_Connecting():
return connecting();case ConnectionState_Connected():
return connected();case ConnectionState_Reconnecting():
return reconnecting(_that.attempt,_that.retryInMs);case ConnectionState_Failed():
return failed(_that.reason,_that.message);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function()?  connecting,TResult? Function()?  connected,TResult? Function( int attempt,  int retryInMs)?  reconnecting,TResult? Function( FailureReason reason,  String message)?  failed,}) {final _that = this;
switch (_that) {
case ConnectionState_Connecting() when connecting != null:
return connecting();case ConnectionState_Connected() when connected != null:
return connected();case ConnectionState_Reconnecting() when reconnecting != null:
return reconnecting(_that.attempt,_that.retryInMs);case ConnectionState_Failed() when failed != null:
return failed(_that.reason,_that.message);case _:
  return null;

}
}

}

/// @nodoc


class ConnectionState_Connecting extends ConnectionState {
  const ConnectionState_Connecting(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ConnectionState_Connecting);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'ConnectionState.connecting()';
}


}




/// @nodoc


class ConnectionState_Connected extends ConnectionState {
  const ConnectionState_Connected(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ConnectionState_Connected);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'ConnectionState.connected()';
}


}




/// @nodoc


class ConnectionState_Reconnecting extends ConnectionState {
  const ConnectionState_Reconnecting({required this.attempt, required this.retryInMs}): super._();
  

 final  int attempt;
 final  int retryInMs;

/// Create a copy of ConnectionState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ConnectionState_ReconnectingCopyWith<ConnectionState_Reconnecting> get copyWith => _$ConnectionState_ReconnectingCopyWithImpl<ConnectionState_Reconnecting>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ConnectionState_Reconnecting&&(identical(other.attempt, attempt) || other.attempt == attempt)&&(identical(other.retryInMs, retryInMs) || other.retryInMs == retryInMs));
}


@override
int get hashCode => Object.hash(runtimeType,attempt,retryInMs);

@override
String toString() {
  return 'ConnectionState.reconnecting(attempt: $attempt, retryInMs: $retryInMs)';
}


}

/// @nodoc
abstract mixin class $ConnectionState_ReconnectingCopyWith<$Res> implements $ConnectionStateCopyWith<$Res> {
  factory $ConnectionState_ReconnectingCopyWith(ConnectionState_Reconnecting value, $Res Function(ConnectionState_Reconnecting) _then) = _$ConnectionState_ReconnectingCopyWithImpl;
@useResult
$Res call({
 int attempt, int retryInMs
});




}
/// @nodoc
class _$ConnectionState_ReconnectingCopyWithImpl<$Res>
    implements $ConnectionState_ReconnectingCopyWith<$Res> {
  _$ConnectionState_ReconnectingCopyWithImpl(this._self, this._then);

  final ConnectionState_Reconnecting _self;
  final $Res Function(ConnectionState_Reconnecting) _then;

/// Create a copy of ConnectionState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? attempt = null,Object? retryInMs = null,}) {
  return _then(ConnectionState_Reconnecting(
attempt: null == attempt ? _self.attempt : attempt // ignore: cast_nullable_to_non_nullable
as int,retryInMs: null == retryInMs ? _self.retryInMs : retryInMs // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

/// @nodoc


class ConnectionState_Failed extends ConnectionState {
  const ConnectionState_Failed({required this.reason, required this.message}): super._();
  

 final  FailureReason reason;
 final  String message;

/// Create a copy of ConnectionState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ConnectionState_FailedCopyWith<ConnectionState_Failed> get copyWith => _$ConnectionState_FailedCopyWithImpl<ConnectionState_Failed>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ConnectionState_Failed&&(identical(other.reason, reason) || other.reason == reason)&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode => Object.hash(runtimeType,reason,message);

@override
String toString() {
  return 'ConnectionState.failed(reason: $reason, message: $message)';
}


}

/// @nodoc
abstract mixin class $ConnectionState_FailedCopyWith<$Res> implements $ConnectionStateCopyWith<$Res> {
  factory $ConnectionState_FailedCopyWith(ConnectionState_Failed value, $Res Function(ConnectionState_Failed) _then) = _$ConnectionState_FailedCopyWithImpl;
@useResult
$Res call({
 FailureReason reason, String message
});




}
/// @nodoc
class _$ConnectionState_FailedCopyWithImpl<$Res>
    implements $ConnectionState_FailedCopyWith<$Res> {
  _$ConnectionState_FailedCopyWithImpl(this._self, this._then);

  final ConnectionState_Failed _self;
  final $Res Function(ConnectionState_Failed) _then;

/// Create a copy of ConnectionState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? reason = null,Object? message = null,}) {
  return _then(ConnectionState_Failed(
reason: null == reason ? _self.reason : reason // ignore: cast_nullable_to_non_nullable
as FailureReason,message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc
mixin _$CoreError {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreError);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'CoreError()';
}


}

/// @nodoc
class $CoreErrorCopyWith<$Res>  {
$CoreErrorCopyWith(CoreError _, $Res Function(CoreError) __);
}


/// Adds pattern-matching-related methods to [CoreError].
extension CoreErrorPatterns on CoreError {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( CoreError_NotInitialized value)?  notInitialized,TResult Function( CoreError_NoIdentity value)?  noIdentity,TResult Function( CoreError_UnknownServer value)?  unknownServer,TResult Function( CoreError_NotConnected value)?  notConnected,TResult Function( CoreError_Timeout value)?  timeout,TResult Function( CoreError_InvalidInput value)?  invalidInput,TResult Function( CoreError_Server value)?  server,TResult Function( CoreError_FingerprintMismatch value)?  fingerprintMismatch,TResult Function( CoreError_Rejected value)?  rejected,TResult Function( CoreError_Connection value)?  connection,TResult Function( CoreError_Storage value)?  storage,required TResult orElse(),}){
final _that = this;
switch (_that) {
case CoreError_NotInitialized() when notInitialized != null:
return notInitialized(_that);case CoreError_NoIdentity() when noIdentity != null:
return noIdentity(_that);case CoreError_UnknownServer() when unknownServer != null:
return unknownServer(_that);case CoreError_NotConnected() when notConnected != null:
return notConnected(_that);case CoreError_Timeout() when timeout != null:
return timeout(_that);case CoreError_InvalidInput() when invalidInput != null:
return invalidInput(_that);case CoreError_Server() when server != null:
return server(_that);case CoreError_FingerprintMismatch() when fingerprintMismatch != null:
return fingerprintMismatch(_that);case CoreError_Rejected() when rejected != null:
return rejected(_that);case CoreError_Connection() when connection != null:
return connection(_that);case CoreError_Storage() when storage != null:
return storage(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( CoreError_NotInitialized value)  notInitialized,required TResult Function( CoreError_NoIdentity value)  noIdentity,required TResult Function( CoreError_UnknownServer value)  unknownServer,required TResult Function( CoreError_NotConnected value)  notConnected,required TResult Function( CoreError_Timeout value)  timeout,required TResult Function( CoreError_InvalidInput value)  invalidInput,required TResult Function( CoreError_Server value)  server,required TResult Function( CoreError_FingerprintMismatch value)  fingerprintMismatch,required TResult Function( CoreError_Rejected value)  rejected,required TResult Function( CoreError_Connection value)  connection,required TResult Function( CoreError_Storage value)  storage,}){
final _that = this;
switch (_that) {
case CoreError_NotInitialized():
return notInitialized(_that);case CoreError_NoIdentity():
return noIdentity(_that);case CoreError_UnknownServer():
return unknownServer(_that);case CoreError_NotConnected():
return notConnected(_that);case CoreError_Timeout():
return timeout(_that);case CoreError_InvalidInput():
return invalidInput(_that);case CoreError_Server():
return server(_that);case CoreError_FingerprintMismatch():
return fingerprintMismatch(_that);case CoreError_Rejected():
return rejected(_that);case CoreError_Connection():
return connection(_that);case CoreError_Storage():
return storage(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( CoreError_NotInitialized value)?  notInitialized,TResult? Function( CoreError_NoIdentity value)?  noIdentity,TResult? Function( CoreError_UnknownServer value)?  unknownServer,TResult? Function( CoreError_NotConnected value)?  notConnected,TResult? Function( CoreError_Timeout value)?  timeout,TResult? Function( CoreError_InvalidInput value)?  invalidInput,TResult? Function( CoreError_Server value)?  server,TResult? Function( CoreError_FingerprintMismatch value)?  fingerprintMismatch,TResult? Function( CoreError_Rejected value)?  rejected,TResult? Function( CoreError_Connection value)?  connection,TResult? Function( CoreError_Storage value)?  storage,}){
final _that = this;
switch (_that) {
case CoreError_NotInitialized() when notInitialized != null:
return notInitialized(_that);case CoreError_NoIdentity() when noIdentity != null:
return noIdentity(_that);case CoreError_UnknownServer() when unknownServer != null:
return unknownServer(_that);case CoreError_NotConnected() when notConnected != null:
return notConnected(_that);case CoreError_Timeout() when timeout != null:
return timeout(_that);case CoreError_InvalidInput() when invalidInput != null:
return invalidInput(_that);case CoreError_Server() when server != null:
return server(_that);case CoreError_FingerprintMismatch() when fingerprintMismatch != null:
return fingerprintMismatch(_that);case CoreError_Rejected() when rejected != null:
return rejected(_that);case CoreError_Connection() when connection != null:
return connection(_that);case CoreError_Storage() when storage != null:
return storage(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function()?  notInitialized,TResult Function()?  noIdentity,TResult Function()?  unknownServer,TResult Function()?  notConnected,TResult Function()?  timeout,TResult Function( String message)?  invalidInput,TResult Function( ErrorCode code,  String message,  int? retryAfterMs)?  server,TResult Function( String expected,  String presented)?  fingerprintMismatch,TResult Function( FailureReason reason,  String message)?  rejected,TResult Function( String message)?  connection,TResult Function( String message)?  storage,required TResult orElse(),}) {final _that = this;
switch (_that) {
case CoreError_NotInitialized() when notInitialized != null:
return notInitialized();case CoreError_NoIdentity() when noIdentity != null:
return noIdentity();case CoreError_UnknownServer() when unknownServer != null:
return unknownServer();case CoreError_NotConnected() when notConnected != null:
return notConnected();case CoreError_Timeout() when timeout != null:
return timeout();case CoreError_InvalidInput() when invalidInput != null:
return invalidInput(_that.message);case CoreError_Server() when server != null:
return server(_that.code,_that.message,_that.retryAfterMs);case CoreError_FingerprintMismatch() when fingerprintMismatch != null:
return fingerprintMismatch(_that.expected,_that.presented);case CoreError_Rejected() when rejected != null:
return rejected(_that.reason,_that.message);case CoreError_Connection() when connection != null:
return connection(_that.message);case CoreError_Storage() when storage != null:
return storage(_that.message);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function()  notInitialized,required TResult Function()  noIdentity,required TResult Function()  unknownServer,required TResult Function()  notConnected,required TResult Function()  timeout,required TResult Function( String message)  invalidInput,required TResult Function( ErrorCode code,  String message,  int? retryAfterMs)  server,required TResult Function( String expected,  String presented)  fingerprintMismatch,required TResult Function( FailureReason reason,  String message)  rejected,required TResult Function( String message)  connection,required TResult Function( String message)  storage,}) {final _that = this;
switch (_that) {
case CoreError_NotInitialized():
return notInitialized();case CoreError_NoIdentity():
return noIdentity();case CoreError_UnknownServer():
return unknownServer();case CoreError_NotConnected():
return notConnected();case CoreError_Timeout():
return timeout();case CoreError_InvalidInput():
return invalidInput(_that.message);case CoreError_Server():
return server(_that.code,_that.message,_that.retryAfterMs);case CoreError_FingerprintMismatch():
return fingerprintMismatch(_that.expected,_that.presented);case CoreError_Rejected():
return rejected(_that.reason,_that.message);case CoreError_Connection():
return connection(_that.message);case CoreError_Storage():
return storage(_that.message);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function()?  notInitialized,TResult? Function()?  noIdentity,TResult? Function()?  unknownServer,TResult? Function()?  notConnected,TResult? Function()?  timeout,TResult? Function( String message)?  invalidInput,TResult? Function( ErrorCode code,  String message,  int? retryAfterMs)?  server,TResult? Function( String expected,  String presented)?  fingerprintMismatch,TResult? Function( FailureReason reason,  String message)?  rejected,TResult? Function( String message)?  connection,TResult? Function( String message)?  storage,}) {final _that = this;
switch (_that) {
case CoreError_NotInitialized() when notInitialized != null:
return notInitialized();case CoreError_NoIdentity() when noIdentity != null:
return noIdentity();case CoreError_UnknownServer() when unknownServer != null:
return unknownServer();case CoreError_NotConnected() when notConnected != null:
return notConnected();case CoreError_Timeout() when timeout != null:
return timeout();case CoreError_InvalidInput() when invalidInput != null:
return invalidInput(_that.message);case CoreError_Server() when server != null:
return server(_that.code,_that.message,_that.retryAfterMs);case CoreError_FingerprintMismatch() when fingerprintMismatch != null:
return fingerprintMismatch(_that.expected,_that.presented);case CoreError_Rejected() when rejected != null:
return rejected(_that.reason,_that.message);case CoreError_Connection() when connection != null:
return connection(_that.message);case CoreError_Storage() when storage != null:
return storage(_that.message);case _:
  return null;

}
}

}

/// @nodoc


class CoreError_NotInitialized extends CoreError {
  const CoreError_NotInitialized(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreError_NotInitialized);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'CoreError.notInitialized()';
}


}




/// @nodoc


class CoreError_NoIdentity extends CoreError {
  const CoreError_NoIdentity(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreError_NoIdentity);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'CoreError.noIdentity()';
}


}




/// @nodoc


class CoreError_UnknownServer extends CoreError {
  const CoreError_UnknownServer(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreError_UnknownServer);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'CoreError.unknownServer()';
}


}




/// @nodoc


class CoreError_NotConnected extends CoreError {
  const CoreError_NotConnected(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreError_NotConnected);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'CoreError.notConnected()';
}


}




/// @nodoc


class CoreError_Timeout extends CoreError {
  const CoreError_Timeout(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreError_Timeout);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'CoreError.timeout()';
}


}




/// @nodoc


class CoreError_InvalidInput extends CoreError {
  const CoreError_InvalidInput({required this.message}): super._();
  

 final  String message;

/// Create a copy of CoreError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreError_InvalidInputCopyWith<CoreError_InvalidInput> get copyWith => _$CoreError_InvalidInputCopyWithImpl<CoreError_InvalidInput>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreError_InvalidInput&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode => Object.hash(runtimeType,message);

@override
String toString() {
  return 'CoreError.invalidInput(message: $message)';
}


}

/// @nodoc
abstract mixin class $CoreError_InvalidInputCopyWith<$Res> implements $CoreErrorCopyWith<$Res> {
  factory $CoreError_InvalidInputCopyWith(CoreError_InvalidInput value, $Res Function(CoreError_InvalidInput) _then) = _$CoreError_InvalidInputCopyWithImpl;
@useResult
$Res call({
 String message
});




}
/// @nodoc
class _$CoreError_InvalidInputCopyWithImpl<$Res>
    implements $CoreError_InvalidInputCopyWith<$Res> {
  _$CoreError_InvalidInputCopyWithImpl(this._self, this._then);

  final CoreError_InvalidInput _self;
  final $Res Function(CoreError_InvalidInput) _then;

/// Create a copy of CoreError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? message = null,}) {
  return _then(CoreError_InvalidInput(
message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class CoreError_Server extends CoreError {
  const CoreError_Server({required this.code, required this.message, this.retryAfterMs}): super._();
  

 final  ErrorCode code;
 final  String message;
 final  int? retryAfterMs;

/// Create a copy of CoreError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreError_ServerCopyWith<CoreError_Server> get copyWith => _$CoreError_ServerCopyWithImpl<CoreError_Server>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreError_Server&&(identical(other.code, code) || other.code == code)&&(identical(other.message, message) || other.message == message)&&(identical(other.retryAfterMs, retryAfterMs) || other.retryAfterMs == retryAfterMs));
}


@override
int get hashCode => Object.hash(runtimeType,code,message,retryAfterMs);

@override
String toString() {
  return 'CoreError.server(code: $code, message: $message, retryAfterMs: $retryAfterMs)';
}


}

/// @nodoc
abstract mixin class $CoreError_ServerCopyWith<$Res> implements $CoreErrorCopyWith<$Res> {
  factory $CoreError_ServerCopyWith(CoreError_Server value, $Res Function(CoreError_Server) _then) = _$CoreError_ServerCopyWithImpl;
@useResult
$Res call({
 ErrorCode code, String message, int? retryAfterMs
});




}
/// @nodoc
class _$CoreError_ServerCopyWithImpl<$Res>
    implements $CoreError_ServerCopyWith<$Res> {
  _$CoreError_ServerCopyWithImpl(this._self, this._then);

  final CoreError_Server _self;
  final $Res Function(CoreError_Server) _then;

/// Create a copy of CoreError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? code = null,Object? message = null,Object? retryAfterMs = freezed,}) {
  return _then(CoreError_Server(
code: null == code ? _self.code : code // ignore: cast_nullable_to_non_nullable
as ErrorCode,message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as String,retryAfterMs: freezed == retryAfterMs ? _self.retryAfterMs : retryAfterMs // ignore: cast_nullable_to_non_nullable
as int?,
  ));
}


}

/// @nodoc


class CoreError_FingerprintMismatch extends CoreError {
  const CoreError_FingerprintMismatch({required this.expected, required this.presented}): super._();
  

 final  String expected;
 final  String presented;

/// Create a copy of CoreError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreError_FingerprintMismatchCopyWith<CoreError_FingerprintMismatch> get copyWith => _$CoreError_FingerprintMismatchCopyWithImpl<CoreError_FingerprintMismatch>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreError_FingerprintMismatch&&(identical(other.expected, expected) || other.expected == expected)&&(identical(other.presented, presented) || other.presented == presented));
}


@override
int get hashCode => Object.hash(runtimeType,expected,presented);

@override
String toString() {
  return 'CoreError.fingerprintMismatch(expected: $expected, presented: $presented)';
}


}

/// @nodoc
abstract mixin class $CoreError_FingerprintMismatchCopyWith<$Res> implements $CoreErrorCopyWith<$Res> {
  factory $CoreError_FingerprintMismatchCopyWith(CoreError_FingerprintMismatch value, $Res Function(CoreError_FingerprintMismatch) _then) = _$CoreError_FingerprintMismatchCopyWithImpl;
@useResult
$Res call({
 String expected, String presented
});




}
/// @nodoc
class _$CoreError_FingerprintMismatchCopyWithImpl<$Res>
    implements $CoreError_FingerprintMismatchCopyWith<$Res> {
  _$CoreError_FingerprintMismatchCopyWithImpl(this._self, this._then);

  final CoreError_FingerprintMismatch _self;
  final $Res Function(CoreError_FingerprintMismatch) _then;

/// Create a copy of CoreError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? expected = null,Object? presented = null,}) {
  return _then(CoreError_FingerprintMismatch(
expected: null == expected ? _self.expected : expected // ignore: cast_nullable_to_non_nullable
as String,presented: null == presented ? _self.presented : presented // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class CoreError_Rejected extends CoreError {
  const CoreError_Rejected({required this.reason, required this.message}): super._();
  

 final  FailureReason reason;
 final  String message;

/// Create a copy of CoreError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreError_RejectedCopyWith<CoreError_Rejected> get copyWith => _$CoreError_RejectedCopyWithImpl<CoreError_Rejected>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreError_Rejected&&(identical(other.reason, reason) || other.reason == reason)&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode => Object.hash(runtimeType,reason,message);

@override
String toString() {
  return 'CoreError.rejected(reason: $reason, message: $message)';
}


}

/// @nodoc
abstract mixin class $CoreError_RejectedCopyWith<$Res> implements $CoreErrorCopyWith<$Res> {
  factory $CoreError_RejectedCopyWith(CoreError_Rejected value, $Res Function(CoreError_Rejected) _then) = _$CoreError_RejectedCopyWithImpl;
@useResult
$Res call({
 FailureReason reason, String message
});




}
/// @nodoc
class _$CoreError_RejectedCopyWithImpl<$Res>
    implements $CoreError_RejectedCopyWith<$Res> {
  _$CoreError_RejectedCopyWithImpl(this._self, this._then);

  final CoreError_Rejected _self;
  final $Res Function(CoreError_Rejected) _then;

/// Create a copy of CoreError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? reason = null,Object? message = null,}) {
  return _then(CoreError_Rejected(
reason: null == reason ? _self.reason : reason // ignore: cast_nullable_to_non_nullable
as FailureReason,message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class CoreError_Connection extends CoreError {
  const CoreError_Connection({required this.message}): super._();
  

 final  String message;

/// Create a copy of CoreError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreError_ConnectionCopyWith<CoreError_Connection> get copyWith => _$CoreError_ConnectionCopyWithImpl<CoreError_Connection>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreError_Connection&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode => Object.hash(runtimeType,message);

@override
String toString() {
  return 'CoreError.connection(message: $message)';
}


}

/// @nodoc
abstract mixin class $CoreError_ConnectionCopyWith<$Res> implements $CoreErrorCopyWith<$Res> {
  factory $CoreError_ConnectionCopyWith(CoreError_Connection value, $Res Function(CoreError_Connection) _then) = _$CoreError_ConnectionCopyWithImpl;
@useResult
$Res call({
 String message
});




}
/// @nodoc
class _$CoreError_ConnectionCopyWithImpl<$Res>
    implements $CoreError_ConnectionCopyWith<$Res> {
  _$CoreError_ConnectionCopyWithImpl(this._self, this._then);

  final CoreError_Connection _self;
  final $Res Function(CoreError_Connection) _then;

/// Create a copy of CoreError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? message = null,}) {
  return _then(CoreError_Connection(
message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class CoreError_Storage extends CoreError {
  const CoreError_Storage({required this.message}): super._();
  

 final  String message;

/// Create a copy of CoreError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreError_StorageCopyWith<CoreError_Storage> get copyWith => _$CoreError_StorageCopyWithImpl<CoreError_Storage>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreError_Storage&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode => Object.hash(runtimeType,message);

@override
String toString() {
  return 'CoreError.storage(message: $message)';
}


}

/// @nodoc
abstract mixin class $CoreError_StorageCopyWith<$Res> implements $CoreErrorCopyWith<$Res> {
  factory $CoreError_StorageCopyWith(CoreError_Storage value, $Res Function(CoreError_Storage) _then) = _$CoreError_StorageCopyWithImpl;
@useResult
$Res call({
 String message
});




}
/// @nodoc
class _$CoreError_StorageCopyWithImpl<$Res>
    implements $CoreError_StorageCopyWith<$Res> {
  _$CoreError_StorageCopyWithImpl(this._self, this._then);

  final CoreError_Storage _self;
  final $Res Function(CoreError_Storage) _then;

/// Create a copy of CoreError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? message = null,}) {
  return _then(CoreError_Storage(
message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc
mixin _$CoreEventPayload {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'CoreEventPayload()';
}


}

/// @nodoc
class $CoreEventPayloadCopyWith<$Res>  {
$CoreEventPayloadCopyWith(CoreEventPayload _, $Res Function(CoreEventPayload) __);
}


/// Adds pattern-matching-related methods to [CoreEventPayload].
extension CoreEventPayloadPatterns on CoreEventPayload {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( CoreEventPayload_ConnectionState value)?  connectionState,TResult Function( CoreEventPayload_Ready value)?  ready,TResult Function( CoreEventPayload_MessageCreate value)?  messageCreate,TResult Function( CoreEventPayload_MessageUpdate value)?  messageUpdate,TResult Function( CoreEventPayload_MessageDelete value)?  messageDelete,TResult Function( CoreEventPayload_ChannelCreate value)?  channelCreate,TResult Function( CoreEventPayload_ChannelUpdate value)?  channelUpdate,TResult Function( CoreEventPayload_ChannelDelete value)?  channelDelete,TResult Function( CoreEventPayload_RoleCreate value)?  roleCreate,TResult Function( CoreEventPayload_RoleUpdate value)?  roleUpdate,TResult Function( CoreEventPayload_RoleDelete value)?  roleDelete,TResult Function( CoreEventPayload_MemberJoin value)?  memberJoin,TResult Function( CoreEventPayload_MemberLeave value)?  memberLeave,TResult Function( CoreEventPayload_MemberUpdate value)?  memberUpdate,TResult Function( CoreEventPayload_PresenceUpdate value)?  presenceUpdate,TResult Function( CoreEventPayload_TypingStart value)?  typingStart,TResult Function( CoreEventPayload_ServerUpdate value)?  serverUpdate,TResult Function( CoreEventPayload_PermissionsUpdate value)?  permissionsUpdate,required TResult orElse(),}){
final _that = this;
switch (_that) {
case CoreEventPayload_ConnectionState() when connectionState != null:
return connectionState(_that);case CoreEventPayload_Ready() when ready != null:
return ready(_that);case CoreEventPayload_MessageCreate() when messageCreate != null:
return messageCreate(_that);case CoreEventPayload_MessageUpdate() when messageUpdate != null:
return messageUpdate(_that);case CoreEventPayload_MessageDelete() when messageDelete != null:
return messageDelete(_that);case CoreEventPayload_ChannelCreate() when channelCreate != null:
return channelCreate(_that);case CoreEventPayload_ChannelUpdate() when channelUpdate != null:
return channelUpdate(_that);case CoreEventPayload_ChannelDelete() when channelDelete != null:
return channelDelete(_that);case CoreEventPayload_RoleCreate() when roleCreate != null:
return roleCreate(_that);case CoreEventPayload_RoleUpdate() when roleUpdate != null:
return roleUpdate(_that);case CoreEventPayload_RoleDelete() when roleDelete != null:
return roleDelete(_that);case CoreEventPayload_MemberJoin() when memberJoin != null:
return memberJoin(_that);case CoreEventPayload_MemberLeave() when memberLeave != null:
return memberLeave(_that);case CoreEventPayload_MemberUpdate() when memberUpdate != null:
return memberUpdate(_that);case CoreEventPayload_PresenceUpdate() when presenceUpdate != null:
return presenceUpdate(_that);case CoreEventPayload_TypingStart() when typingStart != null:
return typingStart(_that);case CoreEventPayload_ServerUpdate() when serverUpdate != null:
return serverUpdate(_that);case CoreEventPayload_PermissionsUpdate() when permissionsUpdate != null:
return permissionsUpdate(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( CoreEventPayload_ConnectionState value)  connectionState,required TResult Function( CoreEventPayload_Ready value)  ready,required TResult Function( CoreEventPayload_MessageCreate value)  messageCreate,required TResult Function( CoreEventPayload_MessageUpdate value)  messageUpdate,required TResult Function( CoreEventPayload_MessageDelete value)  messageDelete,required TResult Function( CoreEventPayload_ChannelCreate value)  channelCreate,required TResult Function( CoreEventPayload_ChannelUpdate value)  channelUpdate,required TResult Function( CoreEventPayload_ChannelDelete value)  channelDelete,required TResult Function( CoreEventPayload_RoleCreate value)  roleCreate,required TResult Function( CoreEventPayload_RoleUpdate value)  roleUpdate,required TResult Function( CoreEventPayload_RoleDelete value)  roleDelete,required TResult Function( CoreEventPayload_MemberJoin value)  memberJoin,required TResult Function( CoreEventPayload_MemberLeave value)  memberLeave,required TResult Function( CoreEventPayload_MemberUpdate value)  memberUpdate,required TResult Function( CoreEventPayload_PresenceUpdate value)  presenceUpdate,required TResult Function( CoreEventPayload_TypingStart value)  typingStart,required TResult Function( CoreEventPayload_ServerUpdate value)  serverUpdate,required TResult Function( CoreEventPayload_PermissionsUpdate value)  permissionsUpdate,}){
final _that = this;
switch (_that) {
case CoreEventPayload_ConnectionState():
return connectionState(_that);case CoreEventPayload_Ready():
return ready(_that);case CoreEventPayload_MessageCreate():
return messageCreate(_that);case CoreEventPayload_MessageUpdate():
return messageUpdate(_that);case CoreEventPayload_MessageDelete():
return messageDelete(_that);case CoreEventPayload_ChannelCreate():
return channelCreate(_that);case CoreEventPayload_ChannelUpdate():
return channelUpdate(_that);case CoreEventPayload_ChannelDelete():
return channelDelete(_that);case CoreEventPayload_RoleCreate():
return roleCreate(_that);case CoreEventPayload_RoleUpdate():
return roleUpdate(_that);case CoreEventPayload_RoleDelete():
return roleDelete(_that);case CoreEventPayload_MemberJoin():
return memberJoin(_that);case CoreEventPayload_MemberLeave():
return memberLeave(_that);case CoreEventPayload_MemberUpdate():
return memberUpdate(_that);case CoreEventPayload_PresenceUpdate():
return presenceUpdate(_that);case CoreEventPayload_TypingStart():
return typingStart(_that);case CoreEventPayload_ServerUpdate():
return serverUpdate(_that);case CoreEventPayload_PermissionsUpdate():
return permissionsUpdate(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( CoreEventPayload_ConnectionState value)?  connectionState,TResult? Function( CoreEventPayload_Ready value)?  ready,TResult? Function( CoreEventPayload_MessageCreate value)?  messageCreate,TResult? Function( CoreEventPayload_MessageUpdate value)?  messageUpdate,TResult? Function( CoreEventPayload_MessageDelete value)?  messageDelete,TResult? Function( CoreEventPayload_ChannelCreate value)?  channelCreate,TResult? Function( CoreEventPayload_ChannelUpdate value)?  channelUpdate,TResult? Function( CoreEventPayload_ChannelDelete value)?  channelDelete,TResult? Function( CoreEventPayload_RoleCreate value)?  roleCreate,TResult? Function( CoreEventPayload_RoleUpdate value)?  roleUpdate,TResult? Function( CoreEventPayload_RoleDelete value)?  roleDelete,TResult? Function( CoreEventPayload_MemberJoin value)?  memberJoin,TResult? Function( CoreEventPayload_MemberLeave value)?  memberLeave,TResult? Function( CoreEventPayload_MemberUpdate value)?  memberUpdate,TResult? Function( CoreEventPayload_PresenceUpdate value)?  presenceUpdate,TResult? Function( CoreEventPayload_TypingStart value)?  typingStart,TResult? Function( CoreEventPayload_ServerUpdate value)?  serverUpdate,TResult? Function( CoreEventPayload_PermissionsUpdate value)?  permissionsUpdate,}){
final _that = this;
switch (_that) {
case CoreEventPayload_ConnectionState() when connectionState != null:
return connectionState(_that);case CoreEventPayload_Ready() when ready != null:
return ready(_that);case CoreEventPayload_MessageCreate() when messageCreate != null:
return messageCreate(_that);case CoreEventPayload_MessageUpdate() when messageUpdate != null:
return messageUpdate(_that);case CoreEventPayload_MessageDelete() when messageDelete != null:
return messageDelete(_that);case CoreEventPayload_ChannelCreate() when channelCreate != null:
return channelCreate(_that);case CoreEventPayload_ChannelUpdate() when channelUpdate != null:
return channelUpdate(_that);case CoreEventPayload_ChannelDelete() when channelDelete != null:
return channelDelete(_that);case CoreEventPayload_RoleCreate() when roleCreate != null:
return roleCreate(_that);case CoreEventPayload_RoleUpdate() when roleUpdate != null:
return roleUpdate(_that);case CoreEventPayload_RoleDelete() when roleDelete != null:
return roleDelete(_that);case CoreEventPayload_MemberJoin() when memberJoin != null:
return memberJoin(_that);case CoreEventPayload_MemberLeave() when memberLeave != null:
return memberLeave(_that);case CoreEventPayload_MemberUpdate() when memberUpdate != null:
return memberUpdate(_that);case CoreEventPayload_PresenceUpdate() when presenceUpdate != null:
return presenceUpdate(_that);case CoreEventPayload_TypingStart() when typingStart != null:
return typingStart(_that);case CoreEventPayload_ServerUpdate() when serverUpdate != null:
return serverUpdate(_that);case CoreEventPayload_PermissionsUpdate() when permissionsUpdate != null:
return permissionsUpdate(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( ConnectionState field0)?  connectionState,TResult Function( ReadySnapshot field0)?  ready,TResult Function( Message field0)?  messageCreate,TResult Function( Message field0)?  messageUpdate,TResult Function( PlatformInt64 channelId,  PlatformInt64 messageId)?  messageDelete,TResult Function( Channel field0)?  channelCreate,TResult Function( Channel field0)?  channelUpdate,TResult Function( PlatformInt64 channelId)?  channelDelete,TResult Function( Role field0)?  roleCreate,TResult Function( Role field0)?  roleUpdate,TResult Function( PlatformInt64 roleId)?  roleDelete,TResult Function( Member field0)?  memberJoin,TResult Function( PlatformInt64 userId)?  memberLeave,TResult Function( Member field0)?  memberUpdate,TResult Function( Presence field0)?  presenceUpdate,TResult Function( PlatformInt64 channelId,  PlatformInt64 userId)?  typingStart,TResult Function( ServerInfo field0)?  serverUpdate,TResult Function( PlatformInt64 serverPermissions,  List<ChannelPermissions> channelPermissions)?  permissionsUpdate,required TResult orElse(),}) {final _that = this;
switch (_that) {
case CoreEventPayload_ConnectionState() when connectionState != null:
return connectionState(_that.field0);case CoreEventPayload_Ready() when ready != null:
return ready(_that.field0);case CoreEventPayload_MessageCreate() when messageCreate != null:
return messageCreate(_that.field0);case CoreEventPayload_MessageUpdate() when messageUpdate != null:
return messageUpdate(_that.field0);case CoreEventPayload_MessageDelete() when messageDelete != null:
return messageDelete(_that.channelId,_that.messageId);case CoreEventPayload_ChannelCreate() when channelCreate != null:
return channelCreate(_that.field0);case CoreEventPayload_ChannelUpdate() when channelUpdate != null:
return channelUpdate(_that.field0);case CoreEventPayload_ChannelDelete() when channelDelete != null:
return channelDelete(_that.channelId);case CoreEventPayload_RoleCreate() when roleCreate != null:
return roleCreate(_that.field0);case CoreEventPayload_RoleUpdate() when roleUpdate != null:
return roleUpdate(_that.field0);case CoreEventPayload_RoleDelete() when roleDelete != null:
return roleDelete(_that.roleId);case CoreEventPayload_MemberJoin() when memberJoin != null:
return memberJoin(_that.field0);case CoreEventPayload_MemberLeave() when memberLeave != null:
return memberLeave(_that.userId);case CoreEventPayload_MemberUpdate() when memberUpdate != null:
return memberUpdate(_that.field0);case CoreEventPayload_PresenceUpdate() when presenceUpdate != null:
return presenceUpdate(_that.field0);case CoreEventPayload_TypingStart() when typingStart != null:
return typingStart(_that.channelId,_that.userId);case CoreEventPayload_ServerUpdate() when serverUpdate != null:
return serverUpdate(_that.field0);case CoreEventPayload_PermissionsUpdate() when permissionsUpdate != null:
return permissionsUpdate(_that.serverPermissions,_that.channelPermissions);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( ConnectionState field0)  connectionState,required TResult Function( ReadySnapshot field0)  ready,required TResult Function( Message field0)  messageCreate,required TResult Function( Message field0)  messageUpdate,required TResult Function( PlatformInt64 channelId,  PlatformInt64 messageId)  messageDelete,required TResult Function( Channel field0)  channelCreate,required TResult Function( Channel field0)  channelUpdate,required TResult Function( PlatformInt64 channelId)  channelDelete,required TResult Function( Role field0)  roleCreate,required TResult Function( Role field0)  roleUpdate,required TResult Function( PlatformInt64 roleId)  roleDelete,required TResult Function( Member field0)  memberJoin,required TResult Function( PlatformInt64 userId)  memberLeave,required TResult Function( Member field0)  memberUpdate,required TResult Function( Presence field0)  presenceUpdate,required TResult Function( PlatformInt64 channelId,  PlatformInt64 userId)  typingStart,required TResult Function( ServerInfo field0)  serverUpdate,required TResult Function( PlatformInt64 serverPermissions,  List<ChannelPermissions> channelPermissions)  permissionsUpdate,}) {final _that = this;
switch (_that) {
case CoreEventPayload_ConnectionState():
return connectionState(_that.field0);case CoreEventPayload_Ready():
return ready(_that.field0);case CoreEventPayload_MessageCreate():
return messageCreate(_that.field0);case CoreEventPayload_MessageUpdate():
return messageUpdate(_that.field0);case CoreEventPayload_MessageDelete():
return messageDelete(_that.channelId,_that.messageId);case CoreEventPayload_ChannelCreate():
return channelCreate(_that.field0);case CoreEventPayload_ChannelUpdate():
return channelUpdate(_that.field0);case CoreEventPayload_ChannelDelete():
return channelDelete(_that.channelId);case CoreEventPayload_RoleCreate():
return roleCreate(_that.field0);case CoreEventPayload_RoleUpdate():
return roleUpdate(_that.field0);case CoreEventPayload_RoleDelete():
return roleDelete(_that.roleId);case CoreEventPayload_MemberJoin():
return memberJoin(_that.field0);case CoreEventPayload_MemberLeave():
return memberLeave(_that.userId);case CoreEventPayload_MemberUpdate():
return memberUpdate(_that.field0);case CoreEventPayload_PresenceUpdate():
return presenceUpdate(_that.field0);case CoreEventPayload_TypingStart():
return typingStart(_that.channelId,_that.userId);case CoreEventPayload_ServerUpdate():
return serverUpdate(_that.field0);case CoreEventPayload_PermissionsUpdate():
return permissionsUpdate(_that.serverPermissions,_that.channelPermissions);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( ConnectionState field0)?  connectionState,TResult? Function( ReadySnapshot field0)?  ready,TResult? Function( Message field0)?  messageCreate,TResult? Function( Message field0)?  messageUpdate,TResult? Function( PlatformInt64 channelId,  PlatformInt64 messageId)?  messageDelete,TResult? Function( Channel field0)?  channelCreate,TResult? Function( Channel field0)?  channelUpdate,TResult? Function( PlatformInt64 channelId)?  channelDelete,TResult? Function( Role field0)?  roleCreate,TResult? Function( Role field0)?  roleUpdate,TResult? Function( PlatformInt64 roleId)?  roleDelete,TResult? Function( Member field0)?  memberJoin,TResult? Function( PlatformInt64 userId)?  memberLeave,TResult? Function( Member field0)?  memberUpdate,TResult? Function( Presence field0)?  presenceUpdate,TResult? Function( PlatformInt64 channelId,  PlatformInt64 userId)?  typingStart,TResult? Function( ServerInfo field0)?  serverUpdate,TResult? Function( PlatformInt64 serverPermissions,  List<ChannelPermissions> channelPermissions)?  permissionsUpdate,}) {final _that = this;
switch (_that) {
case CoreEventPayload_ConnectionState() when connectionState != null:
return connectionState(_that.field0);case CoreEventPayload_Ready() when ready != null:
return ready(_that.field0);case CoreEventPayload_MessageCreate() when messageCreate != null:
return messageCreate(_that.field0);case CoreEventPayload_MessageUpdate() when messageUpdate != null:
return messageUpdate(_that.field0);case CoreEventPayload_MessageDelete() when messageDelete != null:
return messageDelete(_that.channelId,_that.messageId);case CoreEventPayload_ChannelCreate() when channelCreate != null:
return channelCreate(_that.field0);case CoreEventPayload_ChannelUpdate() when channelUpdate != null:
return channelUpdate(_that.field0);case CoreEventPayload_ChannelDelete() when channelDelete != null:
return channelDelete(_that.channelId);case CoreEventPayload_RoleCreate() when roleCreate != null:
return roleCreate(_that.field0);case CoreEventPayload_RoleUpdate() when roleUpdate != null:
return roleUpdate(_that.field0);case CoreEventPayload_RoleDelete() when roleDelete != null:
return roleDelete(_that.roleId);case CoreEventPayload_MemberJoin() when memberJoin != null:
return memberJoin(_that.field0);case CoreEventPayload_MemberLeave() when memberLeave != null:
return memberLeave(_that.userId);case CoreEventPayload_MemberUpdate() when memberUpdate != null:
return memberUpdate(_that.field0);case CoreEventPayload_PresenceUpdate() when presenceUpdate != null:
return presenceUpdate(_that.field0);case CoreEventPayload_TypingStart() when typingStart != null:
return typingStart(_that.channelId,_that.userId);case CoreEventPayload_ServerUpdate() when serverUpdate != null:
return serverUpdate(_that.field0);case CoreEventPayload_PermissionsUpdate() when permissionsUpdate != null:
return permissionsUpdate(_that.serverPermissions,_that.channelPermissions);case _:
  return null;

}
}

}

/// @nodoc


class CoreEventPayload_ConnectionState extends CoreEventPayload {
  const CoreEventPayload_ConnectionState(this.field0): super._();
  

 final  ConnectionState field0;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_ConnectionStateCopyWith<CoreEventPayload_ConnectionState> get copyWith => _$CoreEventPayload_ConnectionStateCopyWithImpl<CoreEventPayload_ConnectionState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_ConnectionState&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'CoreEventPayload.connectionState(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_ConnectionStateCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_ConnectionStateCopyWith(CoreEventPayload_ConnectionState value, $Res Function(CoreEventPayload_ConnectionState) _then) = _$CoreEventPayload_ConnectionStateCopyWithImpl;
@useResult
$Res call({
 ConnectionState field0
});


$ConnectionStateCopyWith<$Res> get field0;

}
/// @nodoc
class _$CoreEventPayload_ConnectionStateCopyWithImpl<$Res>
    implements $CoreEventPayload_ConnectionStateCopyWith<$Res> {
  _$CoreEventPayload_ConnectionStateCopyWithImpl(this._self, this._then);

  final CoreEventPayload_ConnectionState _self;
  final $Res Function(CoreEventPayload_ConnectionState) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(CoreEventPayload_ConnectionState(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as ConnectionState,
  ));
}

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$ConnectionStateCopyWith<$Res> get field0 {
  
  return $ConnectionStateCopyWith<$Res>(_self.field0, (value) {
    return _then(_self.copyWith(field0: value));
  });
}
}

/// @nodoc


class CoreEventPayload_Ready extends CoreEventPayload {
  const CoreEventPayload_Ready(this.field0): super._();
  

 final  ReadySnapshot field0;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_ReadyCopyWith<CoreEventPayload_Ready> get copyWith => _$CoreEventPayload_ReadyCopyWithImpl<CoreEventPayload_Ready>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_Ready&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'CoreEventPayload.ready(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_ReadyCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_ReadyCopyWith(CoreEventPayload_Ready value, $Res Function(CoreEventPayload_Ready) _then) = _$CoreEventPayload_ReadyCopyWithImpl;
@useResult
$Res call({
 ReadySnapshot field0
});




}
/// @nodoc
class _$CoreEventPayload_ReadyCopyWithImpl<$Res>
    implements $CoreEventPayload_ReadyCopyWith<$Res> {
  _$CoreEventPayload_ReadyCopyWithImpl(this._self, this._then);

  final CoreEventPayload_Ready _self;
  final $Res Function(CoreEventPayload_Ready) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(CoreEventPayload_Ready(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as ReadySnapshot,
  ));
}


}

/// @nodoc


class CoreEventPayload_MessageCreate extends CoreEventPayload {
  const CoreEventPayload_MessageCreate(this.field0): super._();
  

 final  Message field0;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_MessageCreateCopyWith<CoreEventPayload_MessageCreate> get copyWith => _$CoreEventPayload_MessageCreateCopyWithImpl<CoreEventPayload_MessageCreate>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_MessageCreate&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'CoreEventPayload.messageCreate(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_MessageCreateCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_MessageCreateCopyWith(CoreEventPayload_MessageCreate value, $Res Function(CoreEventPayload_MessageCreate) _then) = _$CoreEventPayload_MessageCreateCopyWithImpl;
@useResult
$Res call({
 Message field0
});




}
/// @nodoc
class _$CoreEventPayload_MessageCreateCopyWithImpl<$Res>
    implements $CoreEventPayload_MessageCreateCopyWith<$Res> {
  _$CoreEventPayload_MessageCreateCopyWithImpl(this._self, this._then);

  final CoreEventPayload_MessageCreate _self;
  final $Res Function(CoreEventPayload_MessageCreate) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(CoreEventPayload_MessageCreate(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as Message,
  ));
}


}

/// @nodoc


class CoreEventPayload_MessageUpdate extends CoreEventPayload {
  const CoreEventPayload_MessageUpdate(this.field0): super._();
  

 final  Message field0;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_MessageUpdateCopyWith<CoreEventPayload_MessageUpdate> get copyWith => _$CoreEventPayload_MessageUpdateCopyWithImpl<CoreEventPayload_MessageUpdate>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_MessageUpdate&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'CoreEventPayload.messageUpdate(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_MessageUpdateCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_MessageUpdateCopyWith(CoreEventPayload_MessageUpdate value, $Res Function(CoreEventPayload_MessageUpdate) _then) = _$CoreEventPayload_MessageUpdateCopyWithImpl;
@useResult
$Res call({
 Message field0
});




}
/// @nodoc
class _$CoreEventPayload_MessageUpdateCopyWithImpl<$Res>
    implements $CoreEventPayload_MessageUpdateCopyWith<$Res> {
  _$CoreEventPayload_MessageUpdateCopyWithImpl(this._self, this._then);

  final CoreEventPayload_MessageUpdate _self;
  final $Res Function(CoreEventPayload_MessageUpdate) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(CoreEventPayload_MessageUpdate(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as Message,
  ));
}


}

/// @nodoc


class CoreEventPayload_MessageDelete extends CoreEventPayload {
  const CoreEventPayload_MessageDelete({required this.channelId, required this.messageId}): super._();
  

 final  PlatformInt64 channelId;
 final  PlatformInt64 messageId;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_MessageDeleteCopyWith<CoreEventPayload_MessageDelete> get copyWith => _$CoreEventPayload_MessageDeleteCopyWithImpl<CoreEventPayload_MessageDelete>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_MessageDelete&&(identical(other.channelId, channelId) || other.channelId == channelId)&&(identical(other.messageId, messageId) || other.messageId == messageId));
}


@override
int get hashCode => Object.hash(runtimeType,channelId,messageId);

@override
String toString() {
  return 'CoreEventPayload.messageDelete(channelId: $channelId, messageId: $messageId)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_MessageDeleteCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_MessageDeleteCopyWith(CoreEventPayload_MessageDelete value, $Res Function(CoreEventPayload_MessageDelete) _then) = _$CoreEventPayload_MessageDeleteCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 channelId, PlatformInt64 messageId
});




}
/// @nodoc
class _$CoreEventPayload_MessageDeleteCopyWithImpl<$Res>
    implements $CoreEventPayload_MessageDeleteCopyWith<$Res> {
  _$CoreEventPayload_MessageDeleteCopyWithImpl(this._self, this._then);

  final CoreEventPayload_MessageDelete _self;
  final $Res Function(CoreEventPayload_MessageDelete) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? channelId = null,Object? messageId = null,}) {
  return _then(CoreEventPayload_MessageDelete(
channelId: null == channelId ? _self.channelId : channelId // ignore: cast_nullable_to_non_nullable
as PlatformInt64,messageId: null == messageId ? _self.messageId : messageId // ignore: cast_nullable_to_non_nullable
as PlatformInt64,
  ));
}


}

/// @nodoc


class CoreEventPayload_ChannelCreate extends CoreEventPayload {
  const CoreEventPayload_ChannelCreate(this.field0): super._();
  

 final  Channel field0;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_ChannelCreateCopyWith<CoreEventPayload_ChannelCreate> get copyWith => _$CoreEventPayload_ChannelCreateCopyWithImpl<CoreEventPayload_ChannelCreate>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_ChannelCreate&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'CoreEventPayload.channelCreate(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_ChannelCreateCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_ChannelCreateCopyWith(CoreEventPayload_ChannelCreate value, $Res Function(CoreEventPayload_ChannelCreate) _then) = _$CoreEventPayload_ChannelCreateCopyWithImpl;
@useResult
$Res call({
 Channel field0
});




}
/// @nodoc
class _$CoreEventPayload_ChannelCreateCopyWithImpl<$Res>
    implements $CoreEventPayload_ChannelCreateCopyWith<$Res> {
  _$CoreEventPayload_ChannelCreateCopyWithImpl(this._self, this._then);

  final CoreEventPayload_ChannelCreate _self;
  final $Res Function(CoreEventPayload_ChannelCreate) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(CoreEventPayload_ChannelCreate(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as Channel,
  ));
}


}

/// @nodoc


class CoreEventPayload_ChannelUpdate extends CoreEventPayload {
  const CoreEventPayload_ChannelUpdate(this.field0): super._();
  

 final  Channel field0;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_ChannelUpdateCopyWith<CoreEventPayload_ChannelUpdate> get copyWith => _$CoreEventPayload_ChannelUpdateCopyWithImpl<CoreEventPayload_ChannelUpdate>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_ChannelUpdate&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'CoreEventPayload.channelUpdate(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_ChannelUpdateCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_ChannelUpdateCopyWith(CoreEventPayload_ChannelUpdate value, $Res Function(CoreEventPayload_ChannelUpdate) _then) = _$CoreEventPayload_ChannelUpdateCopyWithImpl;
@useResult
$Res call({
 Channel field0
});




}
/// @nodoc
class _$CoreEventPayload_ChannelUpdateCopyWithImpl<$Res>
    implements $CoreEventPayload_ChannelUpdateCopyWith<$Res> {
  _$CoreEventPayload_ChannelUpdateCopyWithImpl(this._self, this._then);

  final CoreEventPayload_ChannelUpdate _self;
  final $Res Function(CoreEventPayload_ChannelUpdate) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(CoreEventPayload_ChannelUpdate(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as Channel,
  ));
}


}

/// @nodoc


class CoreEventPayload_ChannelDelete extends CoreEventPayload {
  const CoreEventPayload_ChannelDelete({required this.channelId}): super._();
  

 final  PlatformInt64 channelId;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_ChannelDeleteCopyWith<CoreEventPayload_ChannelDelete> get copyWith => _$CoreEventPayload_ChannelDeleteCopyWithImpl<CoreEventPayload_ChannelDelete>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_ChannelDelete&&(identical(other.channelId, channelId) || other.channelId == channelId));
}


@override
int get hashCode => Object.hash(runtimeType,channelId);

@override
String toString() {
  return 'CoreEventPayload.channelDelete(channelId: $channelId)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_ChannelDeleteCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_ChannelDeleteCopyWith(CoreEventPayload_ChannelDelete value, $Res Function(CoreEventPayload_ChannelDelete) _then) = _$CoreEventPayload_ChannelDeleteCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 channelId
});




}
/// @nodoc
class _$CoreEventPayload_ChannelDeleteCopyWithImpl<$Res>
    implements $CoreEventPayload_ChannelDeleteCopyWith<$Res> {
  _$CoreEventPayload_ChannelDeleteCopyWithImpl(this._self, this._then);

  final CoreEventPayload_ChannelDelete _self;
  final $Res Function(CoreEventPayload_ChannelDelete) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? channelId = null,}) {
  return _then(CoreEventPayload_ChannelDelete(
channelId: null == channelId ? _self.channelId : channelId // ignore: cast_nullable_to_non_nullable
as PlatformInt64,
  ));
}


}

/// @nodoc


class CoreEventPayload_RoleCreate extends CoreEventPayload {
  const CoreEventPayload_RoleCreate(this.field0): super._();
  

 final  Role field0;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_RoleCreateCopyWith<CoreEventPayload_RoleCreate> get copyWith => _$CoreEventPayload_RoleCreateCopyWithImpl<CoreEventPayload_RoleCreate>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_RoleCreate&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'CoreEventPayload.roleCreate(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_RoleCreateCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_RoleCreateCopyWith(CoreEventPayload_RoleCreate value, $Res Function(CoreEventPayload_RoleCreate) _then) = _$CoreEventPayload_RoleCreateCopyWithImpl;
@useResult
$Res call({
 Role field0
});




}
/// @nodoc
class _$CoreEventPayload_RoleCreateCopyWithImpl<$Res>
    implements $CoreEventPayload_RoleCreateCopyWith<$Res> {
  _$CoreEventPayload_RoleCreateCopyWithImpl(this._self, this._then);

  final CoreEventPayload_RoleCreate _self;
  final $Res Function(CoreEventPayload_RoleCreate) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(CoreEventPayload_RoleCreate(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as Role,
  ));
}


}

/// @nodoc


class CoreEventPayload_RoleUpdate extends CoreEventPayload {
  const CoreEventPayload_RoleUpdate(this.field0): super._();
  

 final  Role field0;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_RoleUpdateCopyWith<CoreEventPayload_RoleUpdate> get copyWith => _$CoreEventPayload_RoleUpdateCopyWithImpl<CoreEventPayload_RoleUpdate>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_RoleUpdate&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'CoreEventPayload.roleUpdate(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_RoleUpdateCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_RoleUpdateCopyWith(CoreEventPayload_RoleUpdate value, $Res Function(CoreEventPayload_RoleUpdate) _then) = _$CoreEventPayload_RoleUpdateCopyWithImpl;
@useResult
$Res call({
 Role field0
});




}
/// @nodoc
class _$CoreEventPayload_RoleUpdateCopyWithImpl<$Res>
    implements $CoreEventPayload_RoleUpdateCopyWith<$Res> {
  _$CoreEventPayload_RoleUpdateCopyWithImpl(this._self, this._then);

  final CoreEventPayload_RoleUpdate _self;
  final $Res Function(CoreEventPayload_RoleUpdate) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(CoreEventPayload_RoleUpdate(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as Role,
  ));
}


}

/// @nodoc


class CoreEventPayload_RoleDelete extends CoreEventPayload {
  const CoreEventPayload_RoleDelete({required this.roleId}): super._();
  

 final  PlatformInt64 roleId;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_RoleDeleteCopyWith<CoreEventPayload_RoleDelete> get copyWith => _$CoreEventPayload_RoleDeleteCopyWithImpl<CoreEventPayload_RoleDelete>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_RoleDelete&&(identical(other.roleId, roleId) || other.roleId == roleId));
}


@override
int get hashCode => Object.hash(runtimeType,roleId);

@override
String toString() {
  return 'CoreEventPayload.roleDelete(roleId: $roleId)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_RoleDeleteCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_RoleDeleteCopyWith(CoreEventPayload_RoleDelete value, $Res Function(CoreEventPayload_RoleDelete) _then) = _$CoreEventPayload_RoleDeleteCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 roleId
});




}
/// @nodoc
class _$CoreEventPayload_RoleDeleteCopyWithImpl<$Res>
    implements $CoreEventPayload_RoleDeleteCopyWith<$Res> {
  _$CoreEventPayload_RoleDeleteCopyWithImpl(this._self, this._then);

  final CoreEventPayload_RoleDelete _self;
  final $Res Function(CoreEventPayload_RoleDelete) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? roleId = null,}) {
  return _then(CoreEventPayload_RoleDelete(
roleId: null == roleId ? _self.roleId : roleId // ignore: cast_nullable_to_non_nullable
as PlatformInt64,
  ));
}


}

/// @nodoc


class CoreEventPayload_MemberJoin extends CoreEventPayload {
  const CoreEventPayload_MemberJoin(this.field0): super._();
  

 final  Member field0;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_MemberJoinCopyWith<CoreEventPayload_MemberJoin> get copyWith => _$CoreEventPayload_MemberJoinCopyWithImpl<CoreEventPayload_MemberJoin>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_MemberJoin&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'CoreEventPayload.memberJoin(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_MemberJoinCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_MemberJoinCopyWith(CoreEventPayload_MemberJoin value, $Res Function(CoreEventPayload_MemberJoin) _then) = _$CoreEventPayload_MemberJoinCopyWithImpl;
@useResult
$Res call({
 Member field0
});




}
/// @nodoc
class _$CoreEventPayload_MemberJoinCopyWithImpl<$Res>
    implements $CoreEventPayload_MemberJoinCopyWith<$Res> {
  _$CoreEventPayload_MemberJoinCopyWithImpl(this._self, this._then);

  final CoreEventPayload_MemberJoin _self;
  final $Res Function(CoreEventPayload_MemberJoin) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(CoreEventPayload_MemberJoin(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as Member,
  ));
}


}

/// @nodoc


class CoreEventPayload_MemberLeave extends CoreEventPayload {
  const CoreEventPayload_MemberLeave({required this.userId}): super._();
  

 final  PlatformInt64 userId;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_MemberLeaveCopyWith<CoreEventPayload_MemberLeave> get copyWith => _$CoreEventPayload_MemberLeaveCopyWithImpl<CoreEventPayload_MemberLeave>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_MemberLeave&&(identical(other.userId, userId) || other.userId == userId));
}


@override
int get hashCode => Object.hash(runtimeType,userId);

@override
String toString() {
  return 'CoreEventPayload.memberLeave(userId: $userId)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_MemberLeaveCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_MemberLeaveCopyWith(CoreEventPayload_MemberLeave value, $Res Function(CoreEventPayload_MemberLeave) _then) = _$CoreEventPayload_MemberLeaveCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 userId
});




}
/// @nodoc
class _$CoreEventPayload_MemberLeaveCopyWithImpl<$Res>
    implements $CoreEventPayload_MemberLeaveCopyWith<$Res> {
  _$CoreEventPayload_MemberLeaveCopyWithImpl(this._self, this._then);

  final CoreEventPayload_MemberLeave _self;
  final $Res Function(CoreEventPayload_MemberLeave) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? userId = null,}) {
  return _then(CoreEventPayload_MemberLeave(
userId: null == userId ? _self.userId : userId // ignore: cast_nullable_to_non_nullable
as PlatformInt64,
  ));
}


}

/// @nodoc


class CoreEventPayload_MemberUpdate extends CoreEventPayload {
  const CoreEventPayload_MemberUpdate(this.field0): super._();
  

 final  Member field0;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_MemberUpdateCopyWith<CoreEventPayload_MemberUpdate> get copyWith => _$CoreEventPayload_MemberUpdateCopyWithImpl<CoreEventPayload_MemberUpdate>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_MemberUpdate&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'CoreEventPayload.memberUpdate(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_MemberUpdateCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_MemberUpdateCopyWith(CoreEventPayload_MemberUpdate value, $Res Function(CoreEventPayload_MemberUpdate) _then) = _$CoreEventPayload_MemberUpdateCopyWithImpl;
@useResult
$Res call({
 Member field0
});




}
/// @nodoc
class _$CoreEventPayload_MemberUpdateCopyWithImpl<$Res>
    implements $CoreEventPayload_MemberUpdateCopyWith<$Res> {
  _$CoreEventPayload_MemberUpdateCopyWithImpl(this._self, this._then);

  final CoreEventPayload_MemberUpdate _self;
  final $Res Function(CoreEventPayload_MemberUpdate) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(CoreEventPayload_MemberUpdate(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as Member,
  ));
}


}

/// @nodoc


class CoreEventPayload_PresenceUpdate extends CoreEventPayload {
  const CoreEventPayload_PresenceUpdate(this.field0): super._();
  

 final  Presence field0;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_PresenceUpdateCopyWith<CoreEventPayload_PresenceUpdate> get copyWith => _$CoreEventPayload_PresenceUpdateCopyWithImpl<CoreEventPayload_PresenceUpdate>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_PresenceUpdate&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'CoreEventPayload.presenceUpdate(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_PresenceUpdateCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_PresenceUpdateCopyWith(CoreEventPayload_PresenceUpdate value, $Res Function(CoreEventPayload_PresenceUpdate) _then) = _$CoreEventPayload_PresenceUpdateCopyWithImpl;
@useResult
$Res call({
 Presence field0
});




}
/// @nodoc
class _$CoreEventPayload_PresenceUpdateCopyWithImpl<$Res>
    implements $CoreEventPayload_PresenceUpdateCopyWith<$Res> {
  _$CoreEventPayload_PresenceUpdateCopyWithImpl(this._self, this._then);

  final CoreEventPayload_PresenceUpdate _self;
  final $Res Function(CoreEventPayload_PresenceUpdate) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(CoreEventPayload_PresenceUpdate(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as Presence,
  ));
}


}

/// @nodoc


class CoreEventPayload_TypingStart extends CoreEventPayload {
  const CoreEventPayload_TypingStart({required this.channelId, required this.userId}): super._();
  

 final  PlatformInt64 channelId;
 final  PlatformInt64 userId;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_TypingStartCopyWith<CoreEventPayload_TypingStart> get copyWith => _$CoreEventPayload_TypingStartCopyWithImpl<CoreEventPayload_TypingStart>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_TypingStart&&(identical(other.channelId, channelId) || other.channelId == channelId)&&(identical(other.userId, userId) || other.userId == userId));
}


@override
int get hashCode => Object.hash(runtimeType,channelId,userId);

@override
String toString() {
  return 'CoreEventPayload.typingStart(channelId: $channelId, userId: $userId)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_TypingStartCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_TypingStartCopyWith(CoreEventPayload_TypingStart value, $Res Function(CoreEventPayload_TypingStart) _then) = _$CoreEventPayload_TypingStartCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 channelId, PlatformInt64 userId
});




}
/// @nodoc
class _$CoreEventPayload_TypingStartCopyWithImpl<$Res>
    implements $CoreEventPayload_TypingStartCopyWith<$Res> {
  _$CoreEventPayload_TypingStartCopyWithImpl(this._self, this._then);

  final CoreEventPayload_TypingStart _self;
  final $Res Function(CoreEventPayload_TypingStart) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? channelId = null,Object? userId = null,}) {
  return _then(CoreEventPayload_TypingStart(
channelId: null == channelId ? _self.channelId : channelId // ignore: cast_nullable_to_non_nullable
as PlatformInt64,userId: null == userId ? _self.userId : userId // ignore: cast_nullable_to_non_nullable
as PlatformInt64,
  ));
}


}

/// @nodoc


class CoreEventPayload_ServerUpdate extends CoreEventPayload {
  const CoreEventPayload_ServerUpdate(this.field0): super._();
  

 final  ServerInfo field0;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_ServerUpdateCopyWith<CoreEventPayload_ServerUpdate> get copyWith => _$CoreEventPayload_ServerUpdateCopyWithImpl<CoreEventPayload_ServerUpdate>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_ServerUpdate&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'CoreEventPayload.serverUpdate(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_ServerUpdateCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_ServerUpdateCopyWith(CoreEventPayload_ServerUpdate value, $Res Function(CoreEventPayload_ServerUpdate) _then) = _$CoreEventPayload_ServerUpdateCopyWithImpl;
@useResult
$Res call({
 ServerInfo field0
});




}
/// @nodoc
class _$CoreEventPayload_ServerUpdateCopyWithImpl<$Res>
    implements $CoreEventPayload_ServerUpdateCopyWith<$Res> {
  _$CoreEventPayload_ServerUpdateCopyWithImpl(this._self, this._then);

  final CoreEventPayload_ServerUpdate _self;
  final $Res Function(CoreEventPayload_ServerUpdate) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(CoreEventPayload_ServerUpdate(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as ServerInfo,
  ));
}


}

/// @nodoc


class CoreEventPayload_PermissionsUpdate extends CoreEventPayload {
  const CoreEventPayload_PermissionsUpdate({required this.serverPermissions, required final  List<ChannelPermissions> channelPermissions}): _channelPermissions = channelPermissions,super._();
  

 final  PlatformInt64 serverPermissions;
 final  List<ChannelPermissions> _channelPermissions;
 List<ChannelPermissions> get channelPermissions {
  if (_channelPermissions is EqualUnmodifiableListView) return _channelPermissions;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_channelPermissions);
}


/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CoreEventPayload_PermissionsUpdateCopyWith<CoreEventPayload_PermissionsUpdate> get copyWith => _$CoreEventPayload_PermissionsUpdateCopyWithImpl<CoreEventPayload_PermissionsUpdate>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is CoreEventPayload_PermissionsUpdate&&(identical(other.serverPermissions, serverPermissions) || other.serverPermissions == serverPermissions)&&const DeepCollectionEquality().equals(other._channelPermissions, _channelPermissions));
}


@override
int get hashCode => Object.hash(runtimeType,serverPermissions,const DeepCollectionEquality().hash(_channelPermissions));

@override
String toString() {
  return 'CoreEventPayload.permissionsUpdate(serverPermissions: $serverPermissions, channelPermissions: $channelPermissions)';
}


}

/// @nodoc
abstract mixin class $CoreEventPayload_PermissionsUpdateCopyWith<$Res> implements $CoreEventPayloadCopyWith<$Res> {
  factory $CoreEventPayload_PermissionsUpdateCopyWith(CoreEventPayload_PermissionsUpdate value, $Res Function(CoreEventPayload_PermissionsUpdate) _then) = _$CoreEventPayload_PermissionsUpdateCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 serverPermissions, List<ChannelPermissions> channelPermissions
});




}
/// @nodoc
class _$CoreEventPayload_PermissionsUpdateCopyWithImpl<$Res>
    implements $CoreEventPayload_PermissionsUpdateCopyWith<$Res> {
  _$CoreEventPayload_PermissionsUpdateCopyWithImpl(this._self, this._then);

  final CoreEventPayload_PermissionsUpdate _self;
  final $Res Function(CoreEventPayload_PermissionsUpdate) _then;

/// Create a copy of CoreEventPayload
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? serverPermissions = null,Object? channelPermissions = null,}) {
  return _then(CoreEventPayload_PermissionsUpdate(
serverPermissions: null == serverPermissions ? _self.serverPermissions : serverPermissions // ignore: cast_nullable_to_non_nullable
as PlatformInt64,channelPermissions: null == channelPermissions ? _self._channelPermissions : channelPermissions // ignore: cast_nullable_to_non_nullable
as List<ChannelPermissions>,
  ));
}


}

// dart format on
