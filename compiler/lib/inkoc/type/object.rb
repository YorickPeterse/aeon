# frozen_string_literal: true

module Inkoc
  module Type
    class Object
      include Inspect
      include Predicates
      include ObjectOperations
      include GenericTypeOperations
      include TypeCompatibility

      attr_reader :attributes, :implemented_traits, :type_parameters,
                  :type_parameter_instances

      attr_accessor :name, :prototype

      def initialize(
        name: Config::OBJECT_CONST,
        prototype: nil,
        type_parameter_instances: {},
        implemented_traits: Set.new
      )
        @name = name
        @prototype = prototype
        @attributes = SymbolTable.new
        @implemented_traits = implemented_traits
        @type_parameters = {}
        @type_parameter_instances = type_parameter_instances
      end

      def new_instance(type_parameter_instances = {})
        self.class.new(
          name: name,
          prototype: self,
          type_parameter_instances: type_parameter_instances,
          implemented_traits: implemented_traits.dup
        )
      end

      def regular_object?
        true
      end

      def type_parameter_instances_compatible?(other)
        return false unless other.regular_object?
        return true if other.type_parameter_instances.empty?

        type_parameter_instances.all? do |name, type|
          other_type = other.lookup_type_parameter_instance(name)

          other_type ? type.type_compatible?(other_type) : false
        end
      end

      def type_compatible?(other)
        valid = super

        if other.regular_object?
          valid && type_parameter_instances_compatible?(other)
        else
          valid
        end
      end
    end
  end
end
