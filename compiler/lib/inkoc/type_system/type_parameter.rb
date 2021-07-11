# frozen_string_literal: true

module Inkoc
  module TypeSystem
    # A type parameter used for generic types.
    class TypeParameter
      include Type
      include NewInstance
      include TypeWithAttributes

      attr_reader :name

      # name - The name of the type parameter.
      # required_traits - The traits required by the type parameter.
      def initialize(name: nil, required_traits: [])
        @name = name

        @required_traits = required_traits.each_with_object({}) do |trait, hash|
          hash[trait.unique_id] = trait
        end
      end

      def required_traits
        @required_traits.values
      end

      def lookup_type_parameter_instance(_)
        nil
      end

      def attributes
        SymbolTable.new
      end

      def empty?
        @required_traits.empty?
      end

      def lookup_method(name)
        @required_traits.each_value do |trait|
          if (symbol = trait.lookup_method(name)) && symbol.any?
            return symbol
          end
        end

        NullSymbol.singleton
      end
      alias lookup_attribute lookup_method

      def new_instance(*)
        self
      end

      def type_parameter?
        true
      end

      def type_name
        if @required_traits.any?
          "#{name}: #{@required_traits.each_value.map(&:type_name).join(' + ')}"
        else
          name
        end
      end

      def type_compatible?(other, state)
        return type_compatible?(other.type, state) if other.reference?
        return true if other.any? || self == other

        if other.type_parameter?
          compatible_with_type_parameter?(other, state)
        elsif other.trait?
          compatible_with_trait?(other, state)
        else
          compatible_with_object?(other)
        end
      end

      def compatible_with_type_parameter?(other, state)
        other.required_traits.all? { |t| compatible_with_trait?(t, state) }
      end

      def compatible_with_trait?(trait, state)
        if @required_traits[trait.unique_id]
          true
        else
          # The trait is not directly required, but might be required indirectly
          # via another required trait.
          @required_traits.each_value.any? do |required|
            required.type_compatible?(trait, state)
          end
        end
      end

      def compatible_with_object?(other)
        return false if @required_traits.empty?

        @required_traits.each_value.all? do |trait|
          trait.prototype_chain_compatible?(other.base_type)
        end
      end

      # Initialises self in the method or self type.
      #
      # This method assumes that self and the given type are type compatible.
      def initialize_as(type, method_type, self_type)
        if method_type.initialize_type_parameter?(self)
          method_type.initialize_type_parameter(self, type)
        elsif self_type.initialize_type_parameter?(self)
          self_type.initialize_type_parameter(self, type)
        end
      end

      def remap_using_method_bounds(block_type)
        block_type.method_bounds[name] || self
      end

      def resolve_type_parameter_with_self(self_type, method_type = nil)
        method_type&.lookup_type_parameter_instance(self) ||
          self_type.lookup_type_parameter_instance(self) ||
          self
      end

      def resolve_type_parameters(self_type, method_type = nil)
        resolve_type_parameter_with_self(self_type, method_type)
      end

      def with_rigid_type_parameters
        RigidTypeParameter.new(self)
      end
    end
  end
end
