# frozen_string_literal: true

module Inkoc
  module TypeSystem
    class RigidTypeParameter
      include Type
      include NewInstance
      include TypeWithAttributes

      attr_reader :type

      def initialize(type)
        @type = type
      end

      def name
        @type.name
      end

      def required_traits
        @type.required_traits
      end

      def lookup_type_parameter_instance(_)
        nil
      end

      def attributes
        @type.attributes
      end

      def empty?
        @type.empty?
      end

      def rigid_type_parameter?
        true
      end

      def lookup_method(name)
        @type.lookup_method(name)
      end
      alias lookup_attribute lookup_method

      def new_instance(*)
        self
      end

      def type_name
        @type.type_name
      end

      def type_compatible?(other, state)
        return type_compatible?(other.type, state) if other.reference?

        if other.rigid_type_parameter?
          @type == other.type # TODO: correct?
          #@type.type_compatible?(other.type, state)
        else
          @type.type_compatible?(other, state)
        end
      end

      def initialize_as(type, method_type, self_type)
        @type.initialize_as(type, method_type, self_type)
      end

      def remap_using_method_bounds(block_type)
        block_type.method_bounds[name] || self
      end

      def resolve_type_parameter_with_self(self_type, method_type = nil)
        method_type&.lookup_type_parameter_instance(@type) ||
          self_type.lookup_type_parameter_instance(@type) ||
          self
      end

      def resolve_type_parameters(self_type, method_type = nil)
        resolve_type_parameter_with_self(self_type, method_type)
      end
    end
  end
end
