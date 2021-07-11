# frozen_string_literal: true

module Inkoc
  module TypeSystem
    # An executable block of code.
    class Block
      include Type
      include TypeWithPrototype
      include TypeName
      include GenericType
      include GenericTypeWithInstances
      include TypeWithAttributes
      include NewInstance

      LAMBDA = :lambda
      CLOSURE = :closure
      METHOD = :method

      attr_reader :name, :required_arguments, :type_parameters, :attributes,
                  :method_bounds, :thrown_types

      attr_accessor :prototype, :captures, :last_argument_is_rest, :throw_type,
                    :return_type, :type_parameter_instances, :infer_return_type,
                    :infer_throw_type, :block_type, :self_type, :yield_type,
                    :yields, :arguments, :extern, :moving, :discard_return_value

      def self.closure(prototype, return_type: nil)
        new(
          name: Config::BLOCK_TYPE_NAME,
          prototype: prototype,
          block_type: CLOSURE,
          return_type: return_type
        )
      end

      def self.lambda(prototype, return_type: nil)
        new(
          name: Config::LAMBDA_TYPE_NAME,
          prototype: prototype,
          block_type: LAMBDA,
          return_type: return_type
        )
      end

      def self.named_method(name, prototype)
        new(
          name: name,
          prototype: prototype,
          block_type: METHOD,
          infer_return_type: false,
          infer_throw_type: false
        )
      end

      # name - The name of the block.
      # prototype - The prototype of the block, if any.
      # block_type - The type of block. Valid values are `:closure`, `:lambda`
      #              and `:method`.
      # return_type - The type of the return value.
      def initialize(
        name: Config::BLOCK_TYPE_NAME,
        prototype: nil,
        block_type: CLOSURE,
        return_type: nil,
        throw_type: nil,
        infer_return_type: true,
        infer_throw_type: true
      )
        @name = name
        @prototype = prototype
        @arguments = SymbolTable.new
        @throw_type = throw_type
        @return_type = return_type || Any.new
        @yield_type = nil
        @required_arguments = 0
        @type_parameters = TypeParameterTable.new
        @type_parameter_instances = TypeParameterInstances.new
        @attributes = SymbolTable.new
        @captures = false
        @block_type = block_type
        @last_argument_is_rest = false
        @infer_return_type = infer_return_type
        @infer_throw_type = infer_throw_type
        @method_bounds = TypeParameterTable.new
        @thrown_types = []
        @self_type = Any.new
        @yields = false
        @moving = false
        @discard_return_value = false
      end

      def block?
        true
      end

      def lambda?
        block_type == LAMBDA
      end

      def closure?
        block_type == CLOSURE
      end

      def method?
        block_type == METHOD
      end

      def lambda_or_closure?
        lambda? || closure?
      end

      def infer_arguments_as_unknown?
        lambda_or_closure?
      end

      def infer_throw_type?
        infer_throw_type && !throw_type
      end

      # Returns all the traits implemented by every block.
      def implemented_traits
        prototype&.implemented_traits || {}
      end

      # Returns true if `self` is compatible with the given type.
      #
      # other - The type to compare with.
      # state - An instance of `Inkoc::State`.
      def type_compatible?(other, state)
        return type_compatible?(other.type, state) if other.reference?

        if other.any?
          true
        elsif other.trait?
          implemented_traits.key?(other.unique_id)
        elsif other.block?
          compatible_with_block?(other, state)
        elsif other.type_parameter?
          compatible_with_type_parameter?(other, state)
        else
          prototype_chain_compatible?(other)
        end
      end

      # Returns true if `self` is compatible with the given block.
      #
      # other - An instance of `Inkoc::TypeSystem::Block` to compare with.
      # state - An instance of `Inkoc::State`.
      def compatible_with_block?(other, state)
        compatible_block_type?(other) &&
          compatible_move_semantics?(other) &&
          compatible_rest_argument?(other) &&
          compatible_arguments?(other, state) &&
          compatible_throw_type?(other, state) &&
          compatible_return_type?(other, state) &&
          compatible_yield_type?(other, state)
      end

      # other - An instance of `Inkoc::TypeSystem::Block` to compare with.
      def compatible_rest_argument?(other)
        last_argument_is_rest == other.last_argument_is_rest
      end

      def compatible_move_semantics?(other)
        # an owned `do` can be safely passed to a `move do`, because the
        # ownership is transferred.
        if closure? && (!moving && other.moving)
          return true
        end

        moving == other.moving
      end

      # other - An instance of `Inkoc::TypeSystem::Block` to compare with.
      def compatible_block_type?(other)
        if method?
          other.method?
        elsif lambda?
          other.lambda? || other.closure?
        else
          other.closure?
        end
      end

      # other - An instance of `Inkoc::TypeSystem::Block` to compare with.
      # state - An instance of `Inkoc::State`.
      def compatible_arguments?(other, state)
        return false unless arguments.length == other.arguments.length

        arguments.zip(other.arguments) do |our, their|
          our_type = resolve_type_parameter(our.type)
          their_type = other.resolve_type_parameter(their.type)

          unless our_type.strict_type_compatible?(their_type, state)
            return false
          end
        end

        true
      end

      # other - An instance of `Inkoc::TypeSystem::Block` to compare with.
      # state - An instance of `Inkoc::State`.
      def compatible_throw_type?(other, state)
        if throw_type
          if other.throw_type
            theirs = other.resolve_type_parameter(other.throw_type)

            resolve_type_parameter(throw_type)
              .strict_type_compatible?(theirs, state)
          else
            false
          end
        else
          true
        end
      end

      # other - An instance of `Inkoc::TypeSystem::Block` to compare with.
      # state - An instance of `Inkoc::State`.
      def compatible_return_type?(other, state)
        # If both blocks discard their return value, it doesn't matter what
        # their types are.
        return true if discard_return_value && other.discard_return_value

        theirs = other.resolve_type_parameter(other.return_type)

        resolve_type_parameter(return_type)
          .strict_type_compatible?(theirs, state)
      end

      def compatible_yield_type?(other, state)
        if yield_type && other.yield_type
          theirs = other.resolve_type_parameter(other.yield_type)

          resolve_type_parameter(yield_type)
            .strict_type_compatible?(theirs, state)
        elsif yield_type.nil? && other.yield_type.nil?
          true
        else
          false
        end
      end

      def type_name
        type_name = ''

        type_name += 'move ' if moving
        type_name += name

        if type_parameters.any?
          type_name += " !(#{formatted_type_parameter_names})"
        end

        type_name += " (#{formatted_argument_type_names})" if arguments.any?

        if throw_type
          type_name += " !! #{resolve_type_parameter(throw_type).type_name}"
        end

        if return_type && !yield_type
          type_name += " -> #{resolve_type_parameter(return_type).type_name}"
        end

        if yield_type
          type_name += " => #{resolve_type_parameter(yield_type).type_name}"
        end

        if method_bounds.any?
          type_name += " when #{method_bounds.map(&:type_name).join(', ')}"
        end

        type_name
      end

      def formatted_type_parameter_names
        type_parameters.map(&:type_name).join(', ')
      end

      def formatted_argument_type_names
        arguments
          .map { |sym| resolve_type_parameter(sym.type).type_name }
          .join(', ')
      end

      # Defines arguments for the given Array of types.
      def define_arguments(args)
        args.each_with_index do |arg, index|
          arguments.define(index.to_s, arg)
        end
      end

      def define_required_argument(name, type, mutable = false)
        @required_arguments += 1

        arguments.define(name, type, mutable)
      end

      def define_rest_argument(name, type, mutable = false)
        @last_argument_is_rest = true

        arguments.define(name, type, mutable)
      end

      def define_call_method
        define_attribute(Config::CALL_MESSAGE, with_rigid_type_parameters)
      end

      def lookup_type(name)
        super || lookup_type_parameter(name)
      end

      # Returns the fully resolved/initialised return type of this block.
      #
      # If the return type is just `Self`, we support late binding of it. For
      # any other types (e.g. `Array!(Self)`) we still use early binding.
      def resolved_return_type(self_type)
        return_type
          .resolve_self_type(self_type)
          .resolve_type_parameters(self_type, self)
          .without_empty_type_parameters(self_type, self)
      end

      def resolved_throw_type(self_type)
        throw_type
          .resolve_self_type(self_type)
          .resolve_type_parameters(self_type, self)
          .without_empty_type_parameters(self_type, self)
      end

      def argument_count_range
        max = last_argument_is_rest ? Float::INFINITY : argument_count

        required_arguments..max
      end

      def argument_count
        arguments.length
      end

      def argument_count_without_rest
        amount = argument_count

        if last_argument_is_rest
          amount - 1
        else
          amount
        end
      end

      def uses_type_parameters?
        type_parameters.any?
      end

      def resolve_type_parameter(type)
        if type.type_parameter?
          lookup_type_parameter_instance(type) || type
        else
          type
        end
      end

      def argument_type_at(index, self_type)
        if index >= argument_count_without_rest
          if last_argument_is_rest
            rest_type = arguments
              .last
              .type
              .resolve_type_parameter_with_self(self_type, self)

            [rest_type, true]
          else
            [TypeSystem::Error.new, false]
          end
        else
          [
            arguments[index]
              .type
              .resolve_type_parameter_with_self(self_type, self),
            false
          ]
        end
      end

      def keyword_argument_type(name, self_type)
        symbol = arguments[name]

        return unless symbol.any?

        symbol.type.resolve_type_parameter_with_self(self_type, self)
      end

      def new_instance_for_send(instances = [])
        if uses_type_parameters?
          new_instance(instances)
        else
          self
        end
      end

      # Initialises any type parameters stored in this type.
      #
      # This method assumes that self and the given type are type compatible.
      def initialize_as(type, method_type, self_type)
        type = type.type if type.reference?

        arguments.zip(type.arguments) do |ours, theirs|
          ours.type.initialize_as(theirs.type, method_type, self_type)
        end

        if type.throw_type
          throw_type&.initialize_as(type.throw_type, method_type, self_type)
        end

        return_type.initialize_as(type.return_type, method_type, self_type)
      end

      # Creates a copy of this method and inherits the type parameter instances
      # from the given type.
      def with_type_parameter_instances_from(types)
        instances = TypeParameterInstances.new

        types.each do |type|
          next unless type.generic_type?

          instances.merge!(type.type_parameter_instances)

          next unless type.object?

          type.implemented_traits.each do |_, trait|
            instances.merge!(trait.type_parameter_instances)
          end
        end

        if instances.empty?
          self
        else
          dup.tap do |copy|
            copy.type_parameter_instances = instances
          end
        end
      end

      def lookup_type_parameter(name)
        super
        # TODO: re-enable? method_bounds[name] || super
      end

      def lookup_type_parameter_instance(param)
        instance = super

        if instance&.type_parameter? && param != instance
          lookup_type_parameter_instance(instance)
        else
          instance
        end
      end

      def with_rigid_type_parameters
        super.tap do |copy|
          copy.arguments = SymbolTable.new

          arguments.symbols.each do |symbol|
            new_arg_type = symbol.type.with_rigid_type_parameters

            copy.arguments.define(symbol.name, new_arg_type, symbol.mutable?)
          end

          copy.throw_type =
            copy.throw_type&.with_rigid_type_parameters

          copy.return_type =
            copy.return_type.with_rigid_type_parameters

          copy.yield_type =
            copy.yield_type&.with_rigid_type_parameters
        end
      end

      def inferred_return_type=(type)
        @return_type = type
        meth = @attributes[Config::CALL_MESSAGE]

        meth.type.return_type = type if meth.any?
      end
    end
  end
end
