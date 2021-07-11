# frozen_string_literal: true

module Inkoc
  module TypeSystem
    class Any
      include Type
      include TypeWithPrototype
      include TypeWithAttributes
      include GenericType
      include NewInstance

      def new_instance(*)
        self
      end

      def generic_type?
        false
      end

      def prototype
        nil
      end

      def prototype=(*)
        nil
      end

      def attributes
        SymbolTable.new
      end

      def type_parameters
        TypeParameterTable.new
      end

      def type_parameter_instances
        TypeParameterInstances.new
      end

      def implemented_traits
        {}
      end

      def define_attribute(name, *)
        NullSymbol.singleton
      end

      def lookup_attribute(name)
        NullSymbol.singleton
      end

      def type_compatible?(other, state)
        return type_compatible?(other.type, state) if other.reference?

        other.is_a?(Any) || compatible_with_type_parameter?(other)
      end

      def compatible_with_type_parameter?(other)
        other.type_parameter? && other.required_traits.empty?
      end

      def type_name
        Config::ANY_TYPE
      end

      def resolved_return_type(*)
        self
      end

      def cast_to?(*)
        true
      end

      def any?
        true
      end
    end
  end
end
